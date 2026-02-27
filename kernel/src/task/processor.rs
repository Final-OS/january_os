//! 处理器状态管理
//!
//! 管理每个 CPU 核心的任务状态。

use alloc::sync::Arc;
use crate::sync::Mutex;
use super::task::Task;
use super::arch::__switch;

/// 每个 CPU 的处理器状态
pub struct Processor {
    /// 当前正在运行的任务
    current: Option<Arc<Mutex<Task>>>,
}

impl Processor {
    pub const fn new() -> Self {
        Self { current: None }
    }

    /// 获取当前任务
    pub fn current(&self) -> Option<Arc<Mutex<Task>>> {
        self.current.clone()
    }

    /// 取出当前任务（设为 None）
    pub fn take_current(&mut self) -> Option<Arc<Mutex<Task>>> {
        self.current.take()
    }

    /// 设置当前任务
    pub fn set_current(&mut self, task: Arc<Mutex<Task>>) {
        self.current = Some(task);
    }
}

const PROCESSOR_SLOT_COUNT: usize = if crate::config::MAX_CPUS > 0 {
    crate::config::MAX_CPUS
} else {
    1
};
static PROCESSORS: [Mutex<Processor>; PROCESSOR_SLOT_COUNT] =
    [const { Mutex::new(Processor::new()) }; PROCESSOR_SLOT_COUNT];

#[inline]
fn current_processor_slot_index() -> usize {
    let cpu_id = crate::smp::current_cpu_id();
    if cpu_id < PROCESSOR_SLOT_COUNT {
        cpu_id
    } else {
        0
    }
}

#[inline]
fn current_processor() -> &'static Mutex<Processor> {
    &PROCESSORS[current_processor_slot_index()]
}

/// 获取当前任务
pub fn current_task() -> Option<Arc<Mutex<Task>>> {
    current_processor().lock().current()
}

/// 取出并替换当前 CPU 的 current 任务
pub(crate) fn replace_current_task(next: Arc<Mutex<Task>>) -> Option<Arc<Mutex<Task>>> {
    let mut proc = current_processor().lock();
    let prev = proc.take_current();
    proc.set_current(next);
    prev
}

/// 取出当前 CPU 的 current 任务
pub(crate) fn take_current_task() -> Option<Arc<Mutex<Task>>> {
    current_processor().lock().take_current()
}

/// 执行上下文切换
///
/// 在锁外调用 __switch，避免死锁。
pub(crate) unsafe fn do_switch(
    prev_ctx_ptr: *mut usize,
    next_ctx_ptr: *const usize,
) {
    __switch(prev_ctx_ptr, next_ctx_ptr);
}
