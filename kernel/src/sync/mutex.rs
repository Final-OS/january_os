//! 互斥锁实现
//!
//! 提供独占访问的互斥锁，当前为自旋实现。
//! 未来可扩展为支持睡眠等待（需要调度器支持）。

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// 互斥锁
///
/// 与 SpinLock 类似，但提供更丰富的 API。
/// 当前实现为自旋锁，未来可以扩展为支持阻塞等待。
pub struct Mutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

impl<T> Mutex<T> {
    /// 创建新的互斥锁
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// 获取锁
    ///
    /// 如果锁被持有，则自旋等待。
    pub fn lock(&self) -> MutexGuard<T> {
        // 快速路径：尝试直接获取
        if self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return MutexGuard { mutex: self };
        }

        // 慢速路径：自旋等待
        self.lock_slow()
    }

    #[cold]
    fn lock_slow(&self) -> MutexGuard<T> {
        loop {
            // 等待锁释放
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }

            // 尝试获取
            if self.locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return MutexGuard { mutex: self };
            }
        }
    }

    /// 尝试获取锁（非阻塞）
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        if self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
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
            guard,
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
                guard,
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
    guard: MutexGuard<'a, T>,
    irq_was_enabled: bool,
}

impl<T> Deref for IrqMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self.guard
    }
}

impl<T> DerefMut for IrqMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.guard
    }
}

impl<T> Drop for IrqMutexGuard<'_, T> {
    fn drop(&mut self) {
        // 先释放锁（通过 drop guard）
        // guard 会在这里自动 drop
        
        // 然后恢复中断
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
    let rflags: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) rflags);
    }
    (rflags & (1 << 9)) != 0  // IF 标志位
}

#[inline]
fn disable_interrupts() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
}

#[inline]
fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
}
