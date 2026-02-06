//! 任务管理器
//!
//! 负责任务的创建、销毁和查找。

use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::sync::Mutex;
use super::task::Task;
use super::scheduler::SCHEDULER;
use super::id::Pid;

/// 全局任务管理器
static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager::new());

pub struct TaskManager {
    tasks: Vec<Arc<Mutex<Task>>>,
}

impl TaskManager {
    pub const fn new() -> Self {
        Self {
            tasks: Vec::new(),
        }
    }
    
    pub fn add(&mut self, task: Arc<Mutex<Task>>) {
        self.tasks.push(task);
    }
}

pub fn init() {
    // 可以在这里做一些初始化工作
}

/// 创建内核线程并添加到调度器
pub fn spawn_kernel_thread(name: &str, entry: extern "C" fn()) {
    let task = Task::new_kernel(name, entry);
    let task_ref = Arc::new(Mutex::new(task));
    
    // 添加到任务列表
    TASK_MANAGER.lock().add(task_ref.clone());
    
    // 添加到就绪队列
    SCHEDULER.lock().add_task(task_ref);
}
