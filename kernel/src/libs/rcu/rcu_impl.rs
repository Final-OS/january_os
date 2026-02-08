//! RCU（Read-Copy-Update）简化实现
//!
//! 语义：
//! - 读者侧无锁读取（仅原子计数）
//! - 写者更新时独占，等待当前读者离开后再回收旧值

use alloc::boxed::Box;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

struct WriterSpinLock {
    locked: AtomicBool,
}

impl WriterSpinLock {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> WriterSpinGuard<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }

        WriterSpinGuard { lock: self }
    }

    fn try_lock(&self) -> Option<WriterSpinGuard<'_>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(WriterSpinGuard { lock: self })
        } else {
            None
        }
    }
}

struct WriterSpinGuard<'a> {
    lock: &'a WriterSpinLock,
}

impl Drop for WriterSpinGuard<'_> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

/// RCU 容器
pub struct Rcu<T> {
    current: AtomicPtr<T>,
    readers: AtomicUsize,
    writer_lock: WriterSpinLock,
}

unsafe impl<T: Send + Sync> Sync for Rcu<T> {}
unsafe impl<T: Send + Sync> Send for Rcu<T> {}

impl<T> Rcu<T> {
    /// 创建 RCU 对象
    pub fn new(value: T) -> Self {
        let ptr = Box::into_raw(Box::new(value));
        Self {
            current: AtomicPtr::new(ptr),
            readers: AtomicUsize::new(0),
            writer_lock: WriterSpinLock::new(),
        }
    }

    /// 进入读临界区
    pub fn read(&self) -> RcuReadGuard<'_, T> {
        self.readers.fetch_add(1, Ordering::Acquire);
        let ptr = self.current.load(Ordering::Acquire);
        debug_assert!(!ptr.is_null());
        RcuReadGuard {
            rcu: self,
            ptr,
            _marker: PhantomData,
        }
    }

    /// 当前读者数量
    #[inline]
    pub fn read_count(&self) -> usize {
        self.readers.load(Ordering::Acquire)
    }

    /// 是否处于宽限期空闲状态（无读者）
    #[inline]
    pub fn is_quiescent(&self) -> bool {
        self.read_count() == 0
    }

    /// 等待所有当前读侧离开（同步宽限期）
    #[inline]
    pub fn synchronize(&self) {
        self.wait_for_readers();
    }

    /// 更新当前值，返回旧值
    ///
    /// 行为：
    /// - 先发布新值
    /// - 再等待所有已在读临界区的读者离开
    /// - 最后回收旧值
    pub fn update(&self, value: T) -> T {
        let _guard = self.writer_lock.lock();
        let new_ptr = Box::into_raw(Box::new(value));
        self.publish_and_reclaim(new_ptr)
    }

    /// 非阻塞更新：若写锁暂不可用则返回 `Err(value)`
    pub fn try_update(&self, value: T) -> Result<T, T> {
        let Some(_guard) = self.writer_lock.try_lock() else {
            return Err(value);
        };

        let new_ptr = Box::into_raw(Box::new(value));
        Ok(self.publish_and_reclaim(new_ptr))
    }

    /// 基于当前值计算新值并更新，返回旧值
    pub fn update_with<F>(&self, f: F) -> T
    where
        F: FnOnce(&T) -> T,
    {
        let _guard = self.writer_lock.lock();

        let current_ptr = self.current.load(Ordering::Acquire);
        debug_assert!(!current_ptr.is_null());

        let new_value = unsafe { f(&*current_ptr) };
        let new_ptr = Box::into_raw(Box::new(new_value));

        self.publish_and_reclaim(new_ptr)
    }

    /// 指针级读取（供性能敏感路径使用）
    #[inline]
    pub fn load_ptr(&self) -> *const T {
        self.current.load(Ordering::Acquire)
    }

    /// 获取内部可变引用（要求独占 `&mut self`）
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

    fn wait_for_readers(&self) {
        while self.readers.load(Ordering::Acquire) != 0 {
            core::hint::spin_loop();
        }
    }

    fn publish_and_reclaim(&self, new_ptr: *mut T) -> T {
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);
        debug_assert!(!old_ptr.is_null());
        self.wait_for_readers();
        unsafe { *Box::from_raw(old_ptr) }
    }
}

impl<T> Drop for Rcu<T> {
    fn drop(&mut self) {
        let ptr = self.current.load(Ordering::Relaxed);
        if !ptr.is_null() {
            unsafe {
                drop(Box::from_raw(ptr));
            }
            self.current.store(ptr::null_mut(), Ordering::Relaxed);
        }
    }
}

/// RCU 读侧守卫
pub struct RcuReadGuard<'a, T> {
    rcu: &'a Rcu<T>,
    ptr: *const T,
    _marker: PhantomData<&'a T>,
}

impl<T> RcuReadGuard<'_, T> {
    /// 当前读守卫快照指针
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }
}

impl<T> Deref for RcuReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> Drop for RcuReadGuard<'_, T> {
    fn drop(&mut self) {
        self.rcu.readers.fetch_sub(1, Ordering::Release);
    }
}
