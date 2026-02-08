//! 任务管理器
//!
//! 负责任务的创建、销毁和查找。

use alloc::sync::Arc;
use crate::sync::Mutex;
use crate::libs::rdtree::RadixTree;
use super::task::Task;
use super::scheduler::SCHEDULER;

/// 全局任务管理器
static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager::new());

pub struct TaskManager {
    tasks: RadixTree<Arc<Mutex<Task>>>,
}

impl TaskManager {
    pub const fn new() -> Self {
        Self {
            tasks: RadixTree::new(),
        }
    }

    pub fn add(&mut self, task: Arc<Mutex<Task>>) {
        let pid = task.lock().pid.0;
        self.tasks.insert(pid, task);
    }

    pub fn find_by_pid(&self, pid: usize) -> Option<&Arc<Mutex<Task>>> {
        self.tasks.get(pid)
    }

    pub fn remove(&mut self, pid: usize) -> Option<Arc<Mutex<Task>>> {
        self.tasks.remove(pid)
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }
}

pub fn init() {
    // 可以在这里做一些初始化工作
}

/// 根据 PID 查找任务
pub fn find_task_by_pid(pid: usize) -> Option<Arc<Mutex<Task>>> {
    TASK_MANAGER.lock().find_by_pid(pid).cloned()
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
