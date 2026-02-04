//! 自旋锁实现
//!
//! 适用于短临界区的基本同步原语。

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// 自旋锁
///
/// 一个简单的自旋锁实现，用于保护共享数据。
/// 在等待锁时会自旋（忙等待）。
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
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
        }
    }

    /// 获取锁
    ///
    /// 如果锁已被持有，则自旋等待直到锁可用。
    /// 返回一个 RAII 守卫，在 drop 时自动释放锁。
    pub fn lock(&self) -> SpinLockGuard<T> {
        // 自旋直到获取锁
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // 自旋等待时使用 PAUSE 指令减少 CPU 功耗
            core::hint::spin_loop();
        }

        SpinLockGuard { lock: self }
    }

    /// 尝试获取锁（非阻塞）
    ///
    /// 如果锁可用则获取并返回 Some，否则立即返回 None。
    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
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
        self.lock.locked.store(false, Ordering::Release);
    }
}
