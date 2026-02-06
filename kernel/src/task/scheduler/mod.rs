//! 调度器 (Scheduler)

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use crate::sync::Mutex;
use super::task::{Task, TaskStatus};

/// 全局调度器
pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

/// 调度器
///
/// 简单的 Round-Robin 调度器
pub struct Scheduler {
    /// 就绪队列
    ready_queue: VecDeque<Arc<Mutex<Task>>>,
}

impl Scheduler {
    /// 创建新的调度器
    pub const fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    
    /// 添加任务到就绪队列
    pub fn add_task(&mut self, task: Arc<Mutex<Task>>) {
        task.lock().status = TaskStatus::Ready;
        self.ready_queue.push_back(task);
    }
    
    /// 获取下一个要运行的任务 (Round Robin)
    pub fn pick_next(&mut self) -> Option<Arc<Mutex<Task>>> {
        self.ready_queue.pop_front()
    }
}

/// 调度函数
pub fn schedule() {
    let next_task = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.pick_next()
    };
    
    if let Some(next) = next_task {
        // 获取当前任务 (这里需要 Processor 抽象)
        // 暂时只打印日志
        // crate::kprintln!("Schedule: Switching to task {}", next.lock().name);
        
        // TODO: 执行真正的切换
        // processor::switch_to(next);
    }
}
