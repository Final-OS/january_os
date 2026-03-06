//! 条件变量（最小实现）
//!
//! 当前阶段提供 `wait/notify_one/notify_all` 基础语义，并在等待路径优先让出调度器，
//! 避免纯忙等。

use core::sync::atomic::{AtomicU64, Ordering};

use super::{Mutex, MutexGuard};

pub struct CondVar {
    seq: AtomicU64,
}

impl CondVar {
    pub const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
        }
    }

    pub fn wait<'a, T>(&self, mutex: &'a Mutex<T>, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let seq = self.seq.load(Ordering::Acquire);
        drop(guard);

        while self.seq.load(Ordering::Acquire) == seq {
            wait_for_notification();
        }

        mutex.lock_blocking()
    }

    pub fn wait_while<'a, T, F>(
        &self,
        mutex: &'a Mutex<T>,
        mut guard: MutexGuard<'a, T>,
        mut condition: F,
    ) -> MutexGuard<'a, T>
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut *guard) {
            guard = self.wait(mutex, guard);
        }
        guard
    }

    #[inline]
    pub fn notify_one(&self) {
        self.seq.fetch_add(1, Ordering::Release);
    }

    #[inline]
    pub fn notify_all(&self) {
        self.seq.fetch_add(1, Ordering::Release);
    }
}

impl Default for CondVar {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn wait_for_notification() {
    if crate::interrupt::interrupts_enabled() && crate::task::current_task().is_some() {
        crate::task::sched::schedule();
    } else {
        core::hint::spin_loop();
    }
}
