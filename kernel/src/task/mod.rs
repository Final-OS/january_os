pub mod arch;
pub mod id;
pub mod context;
pub mod task;
pub mod process;
pub mod scheduler;
pub mod manager;
pub mod processor;
pub mod ipc;

pub use task::{Task, TaskStatus};
pub use id::{TaskId, ProcessId};
pub use manager::spawn_kernel_thread;
pub use manager::find_task_by_pid;
pub use processor::current_task;

/// 初始化任务子系统
pub fn init() {
    crate::info!("Initializing Task subsystem...");
    manager::init();
    crate::ok!("Task subsystem initialized.");
}

/// 获取当前任务的进程 ID
pub fn current_pid() -> Option<ProcessId> {
    current_task().map(|t| t.lock().pid)
}

/// 获取当前任务的父进程 ID
pub fn current_ppid() -> Option<ProcessId> {
    current_task().map(|t| t.lock().ppid)
}

/// 获取当前任务的线程 ID (等同于 TaskId)
pub fn current_tid() -> Option<TaskId> {
    current_task().map(|t| t.lock().id)
}

/// 退出当前任务
pub fn exit_current_task(exit_code: i32) {
    if let Some(task) = current_task() {
        let mut t = task.lock();
        t.status = TaskStatus::Exited;
        t.exit_code = Some(exit_code);
    }
}

/// 退出当前进程（退出其所有任务）
pub fn exit_current_process(exit_code: i32) {
    exit_current_task(exit_code);
}

/// 等待子进程退出 (桩实现)
pub fn wait_child(_pid: Option<ProcessId>) -> Option<(ProcessId, i32)> {
    None
}
