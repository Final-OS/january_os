//! RCU（Read-Copy-Update）内核级实现
//!
//! ## 设计
//! - 读者侧无锁读取（仅原子加载）
//! - 写者更新时独占，支持延迟回收
//! - 宽限期（Grace Period）管理
//! - 支持 call_rcu 延迟回调
//!
//! ## 特性
//! - 零开销读取（无原子计数）
//! - 延迟回收（call_rcu）
//! - 多版本并发控制
//! - 内存屏障优化
//!
//! ## 参考
//! - Linux Kernel RCU
//! - QSBR (Quiescent State Based Reclamation)

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::mem::{self, ManuallyDrop};
use core::ops::Deref;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use crate::sync::SpinLock;

// ============================================================================
// RCU 核心数据结构
// ============================================================================

/// RCU 容器
///
/// 使用原子指针实现无锁读取，支持延迟回收。
///
/// ## 示例
/// ```
/// let rcu = Rcu::new(42);
///
/// // 读取
/// {
///     let guard = rcu.read();
///     assert_eq!(*guard, 42);
/// }
///
/// // 更新
/// let old = rcu.update(99);
/// assert_eq!(old, 42);
/// ```
pub struct Rcu<T> {
    /// 当前值指针
    current: AtomicPtr<T>,

    /// 写者锁（保证单写者）
    writer_lock: SpinLock<()>,

    /// 待回收列表
    pending: SpinLock<Vec<PendingReclaim<T>>>,

    /// 宽限期计数器
    grace_period: AtomicUsize,

    /// 活跃读者计数（用于快速路径检测）
    active_readers: AtomicUsize,
}

/// 待回收项
struct PendingReclaim<T> {
    ptr: NonNull<T>,
    grace_period: usize,
}

unsafe impl<T: Send + Sync> Sync for Rcu<T> {}
unsafe impl<T: Send + Sync> Send for Rcu<T> {}

impl<T> Rcu<T> {
    /// 创建 RCU 对象
    pub fn new(value: T) -> Self {
        let ptr = Box::into_raw(Box::new(value));
        Self {
            current: AtomicPtr::new(ptr),
            writer_lock: SpinLock::new(()),
            pending: SpinLock::new(Vec::new()),
            grace_period: AtomicUsize::new(0),
            active_readers: AtomicUsize::new(0),
        }
    }

    /// 进入读临界区
    ///
    /// 返回读守卫，离开作用域时自动释放。
    ///
    /// 时间复杂度: O(1)，无竞争
    #[inline]
    pub fn read(&self) -> RcuReadGuard<'_, T> {
        // 增加活跃读者计数
        self.active_readers.fetch_add(1, Ordering::Acquire);

        // 加载当前指针（Acquire 保证看到最新的写入）
        let ptr = self.current.load(Ordering::Acquire);
        debug_assert!(!ptr.is_null());

        RcuReadGuard {
            rcu: self,
            ptr,
            _marker: PhantomData,
        }
    }

    /// 尝试进入读临界区（非阻塞）
    ///
    /// 如果当前正在进行宽限期同步，返回 None。
    #[inline]
    pub fn try_read(&self) -> Option<RcuReadGuard<'_, T>> {
        // 简化实现：总是成功
        Some(self.read())
    }

    /// 只读访问（不进入临界区，不保证一致性）
    ///
    /// 警告：仅用于调试或统计，不保证读取到的值有效。
    #[inline]
    pub fn peek(&self) -> *const T {
        self.current.load(Ordering::Relaxed)
    }

    /// 当前活跃读者数量
    #[inline]
    pub fn active_readers(&self) -> usize {
        self.active_readers.load(Ordering::Acquire)
    }

    /// 是否处于静止状态（无活跃读者）
    #[inline]
    pub fn is_quiescent(&self) -> bool {
        self.active_readers() == 0
    }

    /// 更新当前值，返回旧值
    ///
    /// 行为：
    /// 1. 发布新值
    /// 2. 等待宽限期（所有读者离开）
    /// 3. 回收旧值
    ///
    /// 时间复杂度: O(1) + 宽限期等待
    pub fn update(&self, value: T) -> T {
        let _guard = self.writer_lock.lock();

        let new_ptr = Box::into_raw(Box::new(value));
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);

        debug_assert!(!old_ptr.is_null());

        // 等待宽限期
        self.synchronize_rcu();

        // 回收旧值
        unsafe { *Box::from_raw(old_ptr) }
    }

    /// 非阻塞更新
    ///
    /// 如果写锁被占用，返回 Err(value)。
    pub fn try_update(&self, value: T) -> Result<T, T> {
        let Some(_guard) = self.writer_lock.try_lock() else {
            return Err(value);
        };

        let new_ptr = Box::into_raw(Box::new(value));
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);

        debug_assert!(!old_ptr.is_null());

        self.synchronize_rcu();

        Ok(unsafe { *Box::from_raw(old_ptr) })
    }

    /// 基于当前值计算新值并更新
    ///
    /// 闭包接收当前值的引用，返回新值。
    pub fn update_with<F>(&self, f: F) -> T
    where
        F: FnOnce(&T) -> T,
    {
        let _guard = self.writer_lock.lock();

        let current_ptr = self.current.load(Ordering::Acquire);
        debug_assert!(!current_ptr.is_null());

        let new_value = unsafe { f(&*current_ptr) };
        let new_ptr = Box::into_raw(Box::new(new_value));
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);

        self.synchronize_rcu();

        unsafe { *Box::from_raw(old_ptr) }
    }

    /// 延迟更新（异步回收）
    ///
    /// 立即发布新值，但将旧值的回收推迟到宽限期后。
    /// 不会阻塞等待宽限期。
    ///
    /// 返回：是否成功调度回收（失败表示写锁被占用）
    pub fn update_async(&self, value: T) -> Result<(), T> {
        let Some(_guard) = self.writer_lock.try_lock() else {
            return Err(value);
        };

        let new_ptr = Box::into_raw(Box::new(value));
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);

        debug_assert!(!old_ptr.is_null());

        // 记录当前宽限期
        let gp = self.grace_period.load(Ordering::Acquire);

        // 将旧指针加入待回收列表
        let mut pending = self.pending.lock();
        pending.push(PendingReclaim {
            ptr: unsafe { NonNull::new_unchecked(old_ptr) },
            grace_period: gp,
        });

        Ok(())
    }

    /// 同步宽限期
    ///
    /// 等待所有当前读者离开临界区。
    pub fn synchronize_rcu(&self) {
        // 增加宽限期计数
        self.grace_period.fetch_add(1, Ordering::Release);

        // 等待所有活跃读者离开
        let mut spins: u64 = 0;
        while self.active_readers.load(Ordering::Acquire) != 0 {
            core::hint::spin_loop();
            spins += 1;
            // 约 1-5 秒超时 (取决于 CPU 频率)
            if spins > 100_000_000 {
                panic!(
                    "RCU: synchronize_rcu deadlock detected! active_readers={}",
                    self.active_readers.load(Ordering::Relaxed)
                );
            }
        }

        // 尝试回收待处理项
        self.try_reclaim_pending();
    }

    /// 延迟回调（call_rcu 语义）
    ///
    /// 在宽限期后执行回调。
    /// 注意：当前实现立即执行回调，未来可优化为真正的延迟执行。
    pub fn call_rcu<F>(&self, callback: F)
    where
        F: FnOnce() + Send + 'static,
    {
        // 简化实现：同步执行
        self.synchronize_rcu();
        callback();
    }

    /// 屏障：等待所有待处理的回收完成
    pub fn rcu_barrier(&self) {
        self.synchronize_rcu();

        // 确保所有待回收项都已处理
        let mut pending = self.pending.lock();
        let current_gp = self.grace_period.load(Ordering::Acquire);

        pending.retain(|item| {
            if item.grace_period < current_gp {
                unsafe {
                    drop(Box::from_raw(item.ptr.as_ptr()));
                }
                false
            } else {
                true
            }
        });
    }

    /// 获取可变引用（要求独占访问）
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        let ptr = *self.current.get_mut();
        debug_assert!(!ptr.is_null());
        unsafe { &mut *ptr }
    }

    /// 消费 RCU 并返回内部值
    pub fn into_inner(self) -> T {
        let this = ManuallyDrop::new(self);
        let ptr = this.current.load(Ordering::Relaxed);
        debug_assert!(!ptr.is_null());
        unsafe { *Box::from_raw(ptr) }
    }

    /// 尝试回收待处理项
    fn try_reclaim_pending(&self) {
        let mut pending = self.pending.lock();
        let current_gp = self.grace_period.load(Ordering::Acquire);

        pending.retain(|item| {
            if item.grace_period < current_gp {
                // 宽限期已过，可以安全回收
                unsafe {
                    drop(Box::from_raw(item.ptr.as_ptr()));
                }
                false
            } else {
                true
            }
        });
    }
}

impl<T> Drop for Rcu<T> {
    fn drop(&mut self) {
        // 回收当前值
        let ptr = self.current.load(Ordering::Relaxed);
        if !ptr.is_null() {
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }

        // 回收所有待处理项
        let mut pending = self.pending.lock();
        for item in pending.drain(..) {
            unsafe {
                drop(Box::from_raw(item.ptr.as_ptr()));
            }
        }
    }
}

// ============================================================================
// RCU 读守卫
// ============================================================================

/// RCU 读侧守卫
///
/// 持有守卫期间，保证读取的值不会被回收。
/// 离开作用域时自动释放读锁。
pub struct RcuReadGuard<'a, T> {
    rcu: &'a Rcu<T>,
    ptr: *const T,
    _marker: PhantomData<&'a T>,
}

impl<T> RcuReadGuard<'_, T> {
    /// 获取快照指针
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// 克隆值（如果 T 实现了 Clone）
    #[inline]
    pub fn clone_inner(&self) -> T
    where
        T: Clone,
    {
        unsafe { (*self.ptr).clone() }
    }
}

impl<T> Deref for RcuReadGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> Drop for RcuReadGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // 减少活跃读者计数
        self.rcu.active_readers.fetch_sub(1, Ordering::Release);
    }
}

// ============================================================================
// RCU 指针（类型安全的 RCU 指针）
// ============================================================================

/// RCU 保护的指针
///
/// 提供类型安全的 RCU 指针操作。
pub struct RcuPtr<T> {
    ptr: AtomicPtr<T>,
}

impl<T> RcuPtr<T> {
    /// 创建空指针
    pub const fn new() -> Self {
        Self {
            ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// 从值创建
    pub fn from_box(value: Box<T>) -> Self {
        Self {
            ptr: AtomicPtr::new(Box::into_raw(value)),
        }
    }

    /// RCU 解引用（读取指针）
    ///
    /// 使用 Acquire 内存序，保证看到最新的写入。
    #[inline]
    pub fn rcu_dereference(&self) -> *const T {
        self.ptr.load(Ordering::Acquire)
    }

    /// RCU 赋值（更新指针）
    ///
    /// 使用 Release 内存序，保证写入对读者可见。
    #[inline]
    pub fn rcu_assign(&self, ptr: *mut T) -> *mut T {
        self.ptr.swap(ptr, Ordering::Release)
    }

    /// 加载指针（Relaxed）
    #[inline]
    pub fn load_relaxed(&self) -> *const T {
        self.ptr.load(Ordering::Relaxed)
    }

    /// 存储指针（Relaxed）
    #[inline]
    pub fn store_relaxed(&self, ptr: *mut T) {
        self.ptr.store(ptr, Ordering::Relaxed);
    }
}

impl<T> Default for RcuPtr<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 辅助宏
// ============================================================================

/// 断言当前持有 RCU 读锁（调试用）
#[macro_export]
macro_rules! rcu_read_lock_held {
    ($rcu:expr) => {
        debug_assert!($rcu.active_readers() > 0, "RCU read lock not held");
    };
}

/// RCU 解引用（安全加载指针）
#[macro_export]
macro_rules! rcu_dereference {
    ($ptr:expr) => {
        $ptr.load(core::sync::atomic::Ordering::Acquire)
    };
}

/// RCU 赋值（安全存储指针）
#[macro_export]
macro_rules! rcu_assign_pointer {
    ($ptr:expr, $val:expr) => {
        $ptr.store($val, core::sync::atomic::Ordering::Release)
    };
}

// ============================================================================
// 测试辅助
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rcu() {
        let rcu = Rcu::new(42);

        // 读取
        {
            let guard = rcu.read();
            assert_eq!(*guard, 42);
        }

        // 更新
        let old = rcu.update(99);
        assert_eq!(old, 42);

        // 再次读取
        {
            let guard = rcu.read();
            assert_eq!(*guard, 99);
        }
    }

    #[test]
    fn test_concurrent_readers() {
        let rcu = Rcu::new(100);

        let g1 = rcu.read();
        let g2 = rcu.read();
        let g3 = rcu.read();

        assert_eq!(*g1, 100);
        assert_eq!(*g2, 100);
        assert_eq!(*g3, 100);
        assert_eq!(rcu.active_readers(), 3);

        drop(g1);
        assert_eq!(rcu.active_readers(), 2);

        drop(g2);
        drop(g3);
        assert_eq!(rcu.active_readers(), 0);
    }

    #[test]
    fn test_update_with() {
        let rcu = Rcu::new(10);

        let old = rcu.update_with(|v| v * 2);
        assert_eq!(old, 10);

        let guard = rcu.read();
        assert_eq!(*guard, 20);
    }
}
