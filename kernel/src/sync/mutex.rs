//! 互斥锁实现
//!
//! 提供独占访问的互斥锁，当前为自旋实现。
//! 未来可扩展为支持睡眠等待（需要调度器支持）。

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 互斥锁
///
/// 与 SpinLock 类似，但提供更丰富的 API。
/// 当前实现为自旋锁，未来可以扩展为支持阻塞等待。
pub struct Mutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
    // 调试信息
    owner: AtomicU32,
    name: &'static str,
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

impl<T> Mutex<T> {
    /// 创建新的互斥锁
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
            owner: AtomicU32::new(u32::MAX),
            name: "Mutex",
        }
    }

    /// 创建一个带名称的互斥锁（用于调试死锁）
    pub const fn with_name(data: T, name: &'static str) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
            owner: AtomicU32::new(u32::MAX),
            name,
        }
    }

    /// 获取锁
    ///
    /// 如果锁被持有，则自旋等待。
    pub fn lock(&self) -> MutexGuard<T> {
        let me = if crate::interrupt::apic_initialized() {
            crate::interrupt::local_apic_id()
        } else {
            0
        };

        // 递归死锁检测
        if self.locked.load(Ordering::Relaxed) && self.owner.load(Ordering::Relaxed) == me {
            panic!(
                "Mutex::lock: Deadlock detected! Recursive locking by CPU {} on '{}'",
                me, self.name
            );
        }

        // 快速路径：尝试直接获取
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            self.owner.store(me, Ordering::Relaxed);
            return MutexGuard { mutex: self };
        }

        // 慢速路径：自旋等待
        self.lock_slow(me)
    }

    /// 获取锁（可调度等待路径）
    ///
    /// 与 `lock()` 不同，竞争时优先通过调度器让出 CPU，避免纯忙等。
    /// 若当前不在可调度上下文，会回退到自旋等待。
    pub fn lock_blocking(&self) -> MutexGuard<T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            wait_for_lock_event();
        }
    }

    #[cold]
    fn lock_slow(&self, me: u32) -> MutexGuard<T> {
        let mut count = 0;
        loop {
            // 等待锁释放
            while self.locked.load(Ordering::Relaxed) {
                count += 1;
                if count > 10_000_000 {
                    let owner = self.owner.load(Ordering::Relaxed);
                    panic!(
                        "Mutex::lock: Deadlock detected! Timeout waiting for '{}' (held by CPU {}) on CPU {}",
                        self.name, owner, me
                    );
                }
                core::hint::spin_loop();
            }

            // 尝试获取
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                self.owner.store(me, Ordering::Relaxed);
                return MutexGuard { mutex: self };
            }
        }
    }

    /// 尝试获取锁（非阻塞）
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        let me = if crate::interrupt::apic_initialized() {
            crate::interrupt::local_apic_id()
        } else {
            0
        };

        if self.locked.load(Ordering::Relaxed) && self.owner.load(Ordering::Relaxed) == me {
            return None;
        }

        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            self.owner.store(me, Ordering::Relaxed);
            Some(MutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// 检查锁是否被持有
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// 获取内部数据的可变引用（不安全）
    ///
    /// # Safety
    ///
    /// 调用者必须确保当前持有锁或没有其他线程访问。
    pub unsafe fn get_mut(&self) -> &mut T {
        &mut *self.data.get()
    }

    /// 消费锁并返回内部数据
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    /// 强制解锁（不安全）
    ///
    /// # Safety
    ///
    /// 调用者必须确保当前确实持有锁。
    pub unsafe fn force_unlock(&self) {
        self.owner.store(u32::MAX, Ordering::Relaxed);
        self.locked.store(false, Ordering::Release);
    }
}

/// 互斥锁守卫
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.owner.store(u32::MAX, Ordering::Relaxed);
        self.mutex.locked.store(false, Ordering::Release);
    }
}

// ============================================================================
// Debug 实现
// ============================================================================

impl<T: core::fmt::Debug> core::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("Mutex").field("data", &*guard).finish(),
            None => f.debug_struct("Mutex").field("data", &"<locked>").finish(),
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

// ============================================================================
// 带中断禁用的互斥锁
// ============================================================================

/// 带中断禁用的互斥锁
///
/// 获取锁时自动禁用中断，释放时恢复。
/// 用于保护可能在中断上下文中访问的数据。
pub struct IrqMutex<T> {
    inner: Mutex<T>,
}

unsafe impl<T: Send> Sync for IrqMutex<T> {}
unsafe impl<T: Send> Send for IrqMutex<T> {}

impl<T> IrqMutex<T> {
    /// 创建新的 IRQ 互斥锁
    pub const fn new(data: T) -> Self {
        Self {
            inner: Mutex::new(data),
        }
    }

    /// 获取锁（禁用中断）
    pub fn lock(&self) -> IrqMutexGuard<T> {
        // 保存并禁用中断
        let irq_enabled = interrupts_enabled();
        if irq_enabled {
            disable_interrupts();
        }

        let guard = self.inner.lock();

        IrqMutexGuard {
            guard: Some(guard),
            irq_was_enabled: irq_enabled,
        }
    }

    /// 尝试获取锁
    pub fn try_lock(&self) -> Option<IrqMutexGuard<T>> {
        let irq_enabled = interrupts_enabled();
        if irq_enabled {
            disable_interrupts();
        }

        match self.inner.try_lock() {
            Some(guard) => Some(IrqMutexGuard {
                guard: Some(guard),
                irq_was_enabled: irq_enabled,
            }),
            None => {
                // 获取失败，恢复中断状态
                if irq_enabled {
                    enable_interrupts();
                }
                None
            }
        }
    }
}

/// IRQ 互斥锁守卫
pub struct IrqMutexGuard<'a, T> {
    guard: Option<MutexGuard<'a, T>>,
    irq_was_enabled: bool,
}

impl<T> Deref for IrqMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self
            .guard
            .as_ref()
            .expect("IrqMutexGuard without inner guard")
    }
}

impl<T> DerefMut for IrqMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self
            .guard
            .as_mut()
            .expect("IrqMutexGuard without inner guard")
    }
}

impl<T> Drop for IrqMutexGuard<'_, T> {
    fn drop(&mut self) {
        // 必须先释放锁，再恢复中断，避免“开中断但锁尚未释放”的竞态窗口。
        if let Some(guard) = self.guard.take() {
            drop(guard);
        }

        if self.irq_was_enabled {
            enable_interrupts();
        }
    }
}

// ============================================================================
// 中断控制辅助函数
// ============================================================================

#[inline]
fn interrupts_enabled() -> bool {
    crate::interrupt::interrupts_enabled()
}

#[inline]
fn disable_interrupts() {
    crate::interrupt::disable_interrupts();
}

#[inline]
fn enable_interrupts() {
    crate::interrupt::enable_interrupts();
}

#[inline]
fn wait_for_lock_event() {
    if interrupts_enabled() && crate::task::current_task().is_some() {
        crate::task::sched::schedule();
    } else {
        core::hint::spin_loop();
    }
}
