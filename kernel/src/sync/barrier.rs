//! 屏障实现
//!
//! 同步多个线程，直到所有线程都到达屏障点。

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// 屏障
///
/// 阻塞所有线程，直到指定数量的线程都到达屏障点。
/// 然后所有线程同时继续执行。
pub struct Barrier {
    /// 需要等待的线程数
    num_threads: u32,
    /// 当前等待的线程数
    count: AtomicU32,
    /// 代数（用于重复使用屏障）
    generation: AtomicU64,
}

impl Barrier {
    /// 创建新的屏障
    ///
    /// `num_threads` 是需要到达屏障的线程数。
    pub const fn new(num_threads: u32) -> Self {
        Self {
            num_threads,
            count: AtomicU32::new(0),
            generation: AtomicU64::new(0),
        }
    }

    /// 等待所有线程到达屏障
    ///
    /// 返回 `BarrierWaitResult`，其中最后到达的线程是 "leader"。
    pub fn wait(&self) -> BarrierWaitResult {
        let current_gen = self.generation.load(Ordering::Relaxed);

        // 增加等待计数
        let prev = self.count.fetch_add(1, Ordering::AcqRel);
        let arrived = prev + 1;

        if arrived == self.num_threads {
            // 最后一个到达的线程
            // 重置计数并进入下一代
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);

            BarrierWaitResult { is_leader: true }
        } else {
            // 等待其他线程
            while self.generation.load(Ordering::Acquire) == current_gen {
                core::hint::spin_loop();
            }

            BarrierWaitResult { is_leader: false }
        }
    }

    /// 获取屏障需要的线程数
    pub fn num_threads(&self) -> u32 {
        self.num_threads
    }

    /// 获取当前等待的线程数
    pub fn waiting(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }
}

impl core::fmt::Debug for Barrier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Barrier")
            .field("num_threads", &self.num_threads)
            .field("waiting", &self.waiting())
            .finish()
    }
}

/// 屏障等待结果
#[derive(Debug, Clone, Copy)]
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    /// 是否是最后到达的线程（leader）
    ///
    /// leader 可以执行一些只需执行一次的操作。
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }
}

// ============================================================================
// 可重置屏障
// ============================================================================

/// 可重置屏障
///
/// 支持动态改变线程数的屏障。
pub struct ResettableBarrier {
    /// 需要等待的线程数
    num_threads: AtomicU32,
    /// 当前等待的线程数
    count: AtomicU32,
    /// 代数
    generation: AtomicU64,
}

impl ResettableBarrier {
    /// 创建可重置屏障
    pub const fn new(num_threads: u32) -> Self {
        Self {
            num_threads: AtomicU32::new(num_threads),
            count: AtomicU32::new(0),
            generation: AtomicU64::new(0),
        }
    }

    /// 等待所有线程到达
    pub fn wait(&self) -> BarrierWaitResult {
        let current_gen = self.generation.load(Ordering::Relaxed);
        let num = self.num_threads.load(Ordering::Relaxed);

        let prev = self.count.fetch_add(1, Ordering::AcqRel);
        let arrived = prev + 1;

        if arrived >= num {
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);

            BarrierWaitResult { is_leader: true }
        } else {
            while self.generation.load(Ordering::Acquire) == current_gen {
                core::hint::spin_loop();
            }

            BarrierWaitResult { is_leader: false }
        }
    }

    /// 重置屏障的线程数
    ///
    /// # Safety
    ///
    /// 只能在没有线程等待时调用。
    pub unsafe fn reset(&self, num_threads: u32) {
        self.num_threads.store(num_threads, Ordering::Release);
        self.count.store(0, Ordering::Release);
    }

    /// 获取当前线程数设置
    pub fn num_threads(&self) -> u32 {
        self.num_threads.load(Ordering::Relaxed)
    }
}

// ============================================================================
// 单次屏障（Latch）
// ============================================================================

/// 倒计时门闩
///
/// 一个只能使用一次的屏障，当计数减到 0 时打开。
/// 与 Barrier 不同，Latch 不可重置，一旦打开就永远打开。
pub struct CountDownLatch {
    count: AtomicU32,
}

impl CountDownLatch {
    /// 创建倒计时门闩
    pub const fn new(count: u32) -> Self {
        Self {
            count: AtomicU32::new(count),
        }
    }

    /// 减少计数
    ///
    /// 当计数达到 0 时，所有等待的线程都会被释放。
    pub fn count_down(&self) {
        let prev = self.count.fetch_sub(1, Ordering::AcqRel);

        // 如果这是最后一个，不需要做额外操作
        // 等待的线程会自动检测到计数为 0
        if prev == 0 {
            // 防止下溢，恢复到 0
            self.count.store(0, Ordering::Release);
        }
    }

    /// 等待计数达到 0
    pub fn wait(&self) {
        while self.count.load(Ordering::Acquire) > 0 {
            core::hint::spin_loop();
        }
    }

    /// 尝试等待（非阻塞）
    pub fn try_wait(&self) -> bool {
        self.count.load(Ordering::Acquire) == 0
    }

    /// 获取当前计数
    pub fn count(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }
}

impl core::fmt::Debug for CountDownLatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CountDownLatch")
            .field("count", &self.count())
            .finish()
    }
}
