//! 读写锁实现
//!
//! 允许多个读者同时访问，但写者独占访问。

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// 读写锁状态常量
const UNLOCKED: u32 = 0;
const WRITER: u32 = 1 << 31;        // 最高位表示写锁
const MAX_READERS: u32 = WRITER - 1; // 最大读者数

/// 读写锁
///
/// 允许多个读者同时访问数据，或单个写者独占访问。
/// - 读锁：多个线程可同时持有
/// - 写锁：独占访问，等待所有读者释放
pub struct RwLock<T> {
    /// 锁状态
    /// - 0: 未锁定
    /// - 1..MAX_READERS: 读者数量
    /// - WRITER: 写锁被持有
    /// - WRITER | n: 写锁被持有，n 个读者在等待（不使用此模式）
    state: AtomicU32,
    waiting_writers: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for RwLock<T> {}
unsafe impl<T: Send> Send for RwLock<T> {}

impl<T> RwLock<T> {
    /// 创建新的读写锁
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicU32::new(UNLOCKED),
            waiting_writers: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// 获取读锁
    ///
    /// 如果写锁被持有，则自旋等待。
    /// 多个读者可以同时持有读锁。
    pub fn read(&self) -> RwLockReadGuard<T> {
        loop {
            let state = self.state.load(Ordering::Relaxed);
            if self.waiting_writers.load(Ordering::Acquire) > 0 {
                core::hint::spin_loop();
                continue;
            }
            
            // 如果没有写者，尝试增加读者计数
            if state < WRITER {
                if state == MAX_READERS {
                    // 读者数量溢出，极端情况
                    panic!("RwLock: too many readers");
                }
                
                if self.state
                    .compare_exchange_weak(
                        state,
                        state + 1,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return RwLockReadGuard { lock: self };
                }
            }
            
            core::hint::spin_loop();
        }
    }

    /// 尝试获取读锁（非阻塞）
    pub fn try_read(&self) -> Option<RwLockReadGuard<T>> {
        let state = self.state.load(Ordering::Relaxed);
        if self.waiting_writers.load(Ordering::Acquire) > 0 {
            return None;
        }
        
        if state < WRITER && state < MAX_READERS {
            if self.state
                .compare_exchange(
                    state,
                    state + 1,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return Some(RwLockReadGuard { lock: self });
            }
        }
        
        None
    }

    /// 获取写锁
    ///
    /// 独占访问，等待所有读者和其他写者释放。
    pub fn write(&self) -> RwLockWriteGuard<T> {
        self.waiting_writers.fetch_add(1, Ordering::AcqRel);
        loop {
            // 尝试从 UNLOCKED 状态获取写锁
            if self.state
                .compare_exchange_weak(
                    UNLOCKED,
                    WRITER,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                self.waiting_writers.fetch_sub(1, Ordering::AcqRel);
                return RwLockWriteGuard { lock: self };
            }
            
            core::hint::spin_loop();
        }
    }

    /// 尝试获取写锁（非阻塞）
    pub fn try_write(&self) -> Option<RwLockWriteGuard<T>> {
        if self.waiting_writers.load(Ordering::Acquire) > 0 {
            return None;
        }
        if self.state
            .compare_exchange(
                UNLOCKED,
                WRITER,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            Some(RwLockWriteGuard { lock: self })
        } else {
            None
        }
    }

    /// 获取内部数据的可变引用（不安全）
    ///
    /// # Safety
    ///
    /// 调用者必须确保没有其他线程正在访问数据。
    pub unsafe fn get_mut(&self) -> &mut T {
        &mut *self.data.get()
    }

    /// 消费锁并返回内部数据
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

/// 读锁守卫
pub struct RwLockReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        // 减少读者计数
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

/// 写锁守卫
pub struct RwLockWriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        // 释放写锁
        self.lock.state.store(UNLOCKED, Ordering::Release);
    }
}

// ============================================================================
// 为守卫实现 Debug trait（如果 T 实现了 Debug）
// ============================================================================

impl<T: core::fmt::Debug> core::fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.try_read() {
            Some(guard) => f.debug_struct("RwLock").field("data", &*guard).finish(),
            None => f.debug_struct("RwLock").field("data", &"<locked>").finish(),
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for RwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}
