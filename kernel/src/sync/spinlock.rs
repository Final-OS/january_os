//! 自旋锁实现
//!
//! 适用于短临界区的基本同步原语。

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 自旋锁
///
/// 一个简单的自旋锁实现，用于保护共享数据。
/// 在等待锁时会自旋（忙等待）。
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
    // 调试信息
    owner: AtomicU32,
    name: &'static str,
}

// SpinLock 可以安全地在线程间共享
unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

impl<T> SpinLock<T> {
    /// 创建一个新的自旋锁
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
            owner: AtomicU32::new(u32::MAX),
            name: "SpinLock",
        }
    }

    /// 创建一个带名称的自旋锁（用于调试死锁）
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
    /// 如果锁已被持有，则自旋等待直到锁可用。
    /// 返回一个 RAII 守卫，在 drop 时自动释放锁。
    pub fn lock(&self) -> SpinLockGuard<T> {
        let me = if crate::interrupt::apic_initialized() {
            crate::interrupt::local_apic_id()
        } else {
            0
        };

        // 递归死锁检测
        if self.locked.load(Ordering::Relaxed) && self.owner.load(Ordering::Relaxed) == me {
            panic!(
                "SpinLock::lock: Deadlock detected! Recursive locking by CPU {} on '{}'",
                me, self.name
            );
        }

        // 自旋直到获取锁
        let mut count = 0;
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // 超时死锁检测 (~10ms - 100ms)
            count += 1;
            if count > 10_000_000 {
                let owner = self.owner.load(Ordering::Relaxed);
                panic!(
                    "SpinLock::lock: Deadlock detected! Timeout waiting for '{}' (held by CPU {}) on CPU {}",
                    self.name, owner, me
                );
            }

            // 自旋等待时使用 PAUSE 指令减少 CPU 功耗
            core::hint::spin_loop();
        }

        self.owner.store(me, Ordering::Relaxed);
        SpinLockGuard { lock: self }
    }

    /// 尝试获取锁（非阻塞）
    ///
    /// 如果锁可用则获取并返回 Some，否则立即返回 None。
    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        let me = if crate::interrupt::apic_initialized() {
            crate::interrupt::local_apic_id()
        } else {
            0
        };

        if self.locked.load(Ordering::Relaxed) && self.owner.load(Ordering::Relaxed) == me {
            // 递归，直接失败
            return None;
        }

        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            self.owner.store(me, Ordering::Relaxed);
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
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

/// 自旋锁守卫
///
/// RAII 守卫，持有时锁处于锁定状态，drop 时自动释放。
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.owner.store(u32::MAX, Ordering::Relaxed);
        self.lock.locked.store(false, Ordering::Release);
    }
}

/// 带中断禁用的自旋锁
///
/// 用于同时会被中断上下文和普通上下文访问的数据。
pub struct IrqSpinLock<T> {
    inner: SpinLock<T>,
}

unsafe impl<T: Send> Sync for IrqSpinLock<T> {}
unsafe impl<T: Send> Send for IrqSpinLock<T> {}

impl<T> IrqSpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: SpinLock::new(data),
        }
    }

    pub const fn with_name(data: T, name: &'static str) -> Self {
        Self {
            inner: SpinLock::with_name(data, name),
        }
    }

    pub fn lock(&self) -> IrqSpinLockGuard<T> {
        let irq_was_enabled = interrupts_enabled();
        if irq_was_enabled {
            disable_interrupts();
        }

        let guard = self.inner.lock();
        IrqSpinLockGuard {
            guard: Some(guard),
            irq_was_enabled,
        }
    }

    pub fn try_lock(&self) -> Option<IrqSpinLockGuard<T>> {
        let irq_was_enabled = interrupts_enabled();
        if irq_was_enabled {
            disable_interrupts();
        }

        match self.inner.try_lock() {
            Some(guard) => Some(IrqSpinLockGuard {
                guard: Some(guard),
                irq_was_enabled,
            }),
            None => {
                if irq_was_enabled {
                    enable_interrupts();
                }
                None
            }
        }
    }
}

pub struct IrqSpinLockGuard<'a, T> {
    guard: Option<SpinLockGuard<'a, T>>,
    irq_was_enabled: bool,
}

impl<T> Deref for IrqSpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self
            .guard
            .as_ref()
            .expect("IrqSpinLockGuard without inner guard")
    }
}

impl<T> DerefMut for IrqSpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self
            .guard
            .as_mut()
            .expect("IrqSpinLockGuard without inner guard")
    }
}

impl<T> Drop for IrqSpinLockGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            drop(guard);
        }

        if self.irq_was_enabled {
            enable_interrupts();
        }
    }
}

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
