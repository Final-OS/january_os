//! 信号量实现
//!
//! 计数信号量，用于控制资源的并发访问数量。

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// 计数信号量
///
/// 维护一个计数器，允许多个线程同时获取（直到计数为 0）。
/// 当计数为 0 时，获取操作会自旋等待。
pub struct Semaphore {
    /// 当前计数（可用资源数）
    count: AtomicI32,
}

impl Semaphore {
    /// 创建指定初始计数的信号量
    pub const fn new(count: i32) -> Self {
        Self {
            count: AtomicI32::new(count),
        }
    }

    /// 创建二元信号量（互斥）
    pub const fn binary() -> Self {
        Self::new(1)
    }

    /// 获取信号量（P 操作 / down / wait）
    ///
    /// 将计数减 1。如果计数为 0，则自旋等待。
    pub fn acquire(&self) {
        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count > 0 {
                if self
                    .count
                    .compare_exchange_weak(count, count - 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
            }

            core::hint::spin_loop();
        }
    }

    /// 获取信号量（可调度等待路径）
    ///
    /// 在可调度上下文优先让出 CPU，避免纯忙等。
    pub fn acquire_blocking(&self) {
        while !self.try_acquire() {
            wait_for_permit();
        }
    }

    /// 尝试获取信号量（非阻塞）
    pub fn try_acquire(&self) -> bool {
        let count = self.count.load(Ordering::Relaxed);

        if count > 0 {
            self.count
                .compare_exchange(count, count - 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        } else {
            false
        }
    }

    /// 获取多个许可
    pub fn acquire_many(&self, permits: u32) {
        let permits = permits as i32;

        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count >= permits {
                if self
                    .count
                    .compare_exchange_weak(
                        count,
                        count - permits,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return;
                }
            }

            core::hint::spin_loop();
        }
    }

    /// 获取多个许可（可调度等待路径）
    pub fn acquire_many_blocking(&self, permits: u32) {
        let permits = permits as i32;

        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count >= permits {
                if self
                    .count
                    .compare_exchange_weak(
                        count,
                        count - permits,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return;
                }
            } else {
                wait_for_permit();
            }
        }
    }

    /// 释放信号量（V 操作 / up / signal）
    ///
    /// 将计数加 1。
    pub fn release(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }

    /// 释放多个许可
    pub fn release_many(&self, permits: u32) {
        self.count.fetch_add(permits as i32, Ordering::Release);
    }

    /// 获取当前计数
    pub fn available(&self) -> i32 {
        self.count.load(Ordering::Relaxed)
    }
}

impl core::fmt::Debug for Semaphore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Semaphore")
            .field("count", &self.available())
            .finish()
    }
}

// ============================================================================
// 带守卫的信号量
// ============================================================================

/// 信号量许可守卫
///
/// RAII 风格的信号量许可，drop 时自动释放。
pub struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
    permits: u32,
}

impl<'a> SemaphorePermit<'a> {
    /// 获取许可
    pub fn acquire(semaphore: &'a Semaphore) -> Self {
        semaphore.acquire();
        Self {
            semaphore,
            permits: 1,
        }
    }

    /// 获取多个许可
    pub fn acquire_many(semaphore: &'a Semaphore, permits: u32) -> Self {
        semaphore.acquire_many(permits);
        Self { semaphore, permits }
    }

    /// 尝试获取许可
    pub fn try_acquire(semaphore: &'a Semaphore) -> Option<Self> {
        if semaphore.try_acquire() {
            Some(Self {
                semaphore,
                permits: 1,
            })
        } else {
            None
        }
    }

    /// 忘记许可（不自动释放）
    pub fn forget(self) {
        core::mem::forget(self);
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.semaphore.release_many(self.permits);
    }
}

// ============================================================================
// 有界信号量
// ============================================================================

/// 有界信号量
///
/// 带有最大值限制的信号量，防止计数超过上限。
pub struct BoundedSemaphore {
    count: AtomicU32,
    max: u32,
}

impl BoundedSemaphore {
    /// 创建有界信号量
    pub const fn new(initial: u32, max: u32) -> Self {
        Self {
            count: AtomicU32::new(initial),
            max,
        }
    }

    /// 获取许可
    pub fn acquire(&self) {
        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count > 0 {
                if self
                    .count
                    .compare_exchange_weak(count, count - 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
            }

            core::hint::spin_loop();
        }
    }

    /// 尝试获取许可
    pub fn try_acquire(&self) -> bool {
        let count = self.count.load(Ordering::Relaxed);

        if count > 0 {
            self.count
                .compare_exchange(count, count - 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        } else {
            false
        }
    }

    /// 释放许可
    ///
    /// 如果计数已达最大值，返回 false。
    pub fn release(&self) -> bool {
        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count >= self.max {
                return false;
            }

            if self
                .count
                .compare_exchange_weak(count, count + 1, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// 获取当前计数
    pub fn available(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }

    /// 获取最大值
    pub fn max(&self) -> u32 {
        self.max
    }
}

impl core::fmt::Debug for BoundedSemaphore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BoundedSemaphore")
            .field("count", &self.available())
            .field("max", &self.max)
            .finish()
    }
}

#[inline]
fn wait_for_permit() {
    if crate::interrupt::interrupts_enabled() && crate::task::current_task().is_some() {
        crate::task::sched::schedule();
    } else {
        core::hint::spin_loop();
    }
}
