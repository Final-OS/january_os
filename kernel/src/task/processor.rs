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

pub(crate) static PROCESSOR: Mutex<Processor> = Mutex::new(Processor::new());

/// 获取当前任务
pub fn current_task() -> Option<Arc<Mutex<Task>>> {
    PROCESSOR.lock().current()
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
