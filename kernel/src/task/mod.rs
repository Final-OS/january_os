pub mod arch;
pub mod context;
pub mod exec;
pub mod id;
pub mod ipc;
pub mod manager;
pub mod process;
pub mod processor;
pub mod scheduler;
pub mod task;

pub use id::{ProcessId, TaskId};
pub use manager::find_process_by_pid;
pub use exec::{
    build_elf_load_plan,
    clear_exec_image_provider,
    load_exec_image,
    preview_pt_load_mapping,
    register_exec_image_provider,
    rollback_exec_mappings,
    stage_pt_load_mappings,
    ExecImageProvider,
    ExecLoadPlan,
    ExecMapPreview,
    ExecMappedPage,
    ExecMappedPageKind,
};
pub use manager::find_task_by_pid;
pub use manager::lookup_current_exec_mapping;
pub use manager::record_current_exec_request;
pub use manager::set_current_exec_mappings;
pub use manager::spawn_kernel_thread;
pub use manager::WaitChildConsumeEvent;
pub use manager::WaitChildOptions;
pub use manager::WaitChildObserveResult;
pub use manager::WaitChildResult;
pub use manager::WaitCloneFilter;
pub use manager::WaitRusageSnapshot;
pub use manager::WaitTarget;
pub use processor::current_task;
pub use task::{Task, TaskStatus};

/// 初始化任务子系统
pub fn init() {
    crate::info!("Initializing Task subsystem...");
    register_exec_image_provider(crate::fs::read_static_file);
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

/// 获取当前任务的进程组 ID
pub fn current_pgid() -> Option<ProcessId> {
    let pid = current_pid()?;
    find_process_by_pid(pid).map(|process| process.lock().pgid)
}

/// 获取当前任务的线程 ID (等同于 TaskId)
pub fn current_tid() -> Option<TaskId> {
    current_task().map(|t| t.lock().id)
}

/// 退出当前任务
pub fn exit_current_task(exit_code: i32) {
    manager::exit_current_task(exit_code);
}

/// 退出当前进程（退出其所有任务）
pub fn exit_current_process(exit_code: i32) {
    manager::exit_current_process(exit_code);
}

/// 等待子进程退出（最小实现：仅回收 Zombie）
pub fn wait_child(pid: Option<ProcessId>) -> Option<(ProcessId, i32)> {
    manager::wait_child(pid)
}

/// 等待子进程退出（返回详细状态）
pub fn wait_child_result(pid: Option<ProcessId>) -> WaitChildResult {
    manager::wait_child_result(pid)
}

/// 按目标等待子进程退出（返回详细状态）
pub fn wait_child_result_by_target(target: WaitTarget) -> WaitChildResult {
    manager::wait_child_result_by_target(target)
}

/// 仅观测子进程等待状态（不回收）
pub fn wait_child_observe_by_target(target: WaitTarget) -> WaitChildObserveResult {
    manager::wait_child_observe_by_target(target)
}

/// 按目标+选项观测子进程等待状态
pub fn wait_child_observe_by_target_with_options(
    target: WaitTarget,
    options: WaitChildOptions,
) -> WaitChildObserveResult {
    manager::wait_child_observe_by_target_with_options(target, options)
}

/// 回收已观测到的 Zombie 子进程
pub fn reap_observed_child(child_pid: ProcessId) -> Option<(ProcessId, i32)> {
    manager::reap_observed_child(child_pid)
}

/// 消费已观测到的等待事件（Stopped / Continued）
pub fn consume_observed_wait_event(child_pid: ProcessId, event: WaitChildConsumeEvent) -> bool {
    manager::consume_observed_wait_event(child_pid, event)
}

/// 获取已观测子进程的 rusage 快照（仅父进程可见）
pub fn snapshot_observed_child_rusage(child_pid: ProcessId) -> Option<WaitRusageSnapshot> {
    manager::snapshot_observed_child_rusage(child_pid)
}
