pub mod arch;
pub mod context;
pub mod id;
pub mod ipc;
pub mod manager;
pub mod process;
pub mod processor;
pub mod scheduler;
pub mod task;

pub use process::exec::{
    build_elf_load_plan, install_current_exec_vmas, preview_pt_load_mapping,
    rollback_exec_mappings, setup_initial_user_stack, stage_pt_load_mappings, ExecLoadPlan,
    ExecMapPreview, ExecMappedPage, ExecMappedPageKind,
};
pub use process::fork::{clone_current, fork_current, vfork_current};
pub use process::wait::WaitEvent;
pub use id::{ProcessId, TaskId};
pub use manager::current_mm_ptr;
pub use manager::find_process_by_pid;
pub use manager::find_task_by_pid;
pub use manager::lookup_current_exec_mapping;
pub use manager::record_current_exec_request;
pub use manager::set_current_exec_mappings;
pub use manager::spawn_kernel_thread;
pub use manager::spawn_kernel_thread_with_mm_mode;
pub use manager::spawn_kernel_thread_with_mm_mode_checked;
pub use manager::SpawnMmMode;
pub use manager::WaitChildConsumeEvent;
pub use manager::WaitChildObserveResult;
pub use manager::WaitChildOptions;
pub use manager::WaitChildResult;
pub use manager::WaitCloneFilter;
pub use manager::WaitRusageSnapshot;
pub use manager::WaitTarget;
pub use processor::current_task;
pub use scheduler::snapshot_stats as scheduler_snapshot_stats;
pub use scheduler::SchedulerStats;
pub use task::{Task, TaskStatus};

#[derive(Debug, Clone, Copy)]
pub struct TaskInitReport {
    pub scheduler_ready: bool,
    pub process_runtime_ready: bool,
}

pub fn init_runtime() -> TaskInitReport {
    crate::info!("[TASK] Initializing Task subsystem...");
    manager::init();
    crate::ok!("[TASK] Task subsystem initialized.");

    TaskInitReport {
        scheduler_ready: true,
        process_runtime_ready: true,
    }
}

/// 初始化任务子系统
pub fn init() {
    let _ = init_runtime();
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
    process::exit::exit_current_task(exit_code);
}

/// 退出当前进程（退出其所有任务）
pub fn exit_current_process(exit_code: i32) {
    process::exit::exit_current_process(exit_code);
}

/// 等待子进程退出（最小实现：仅回收 Zombie）
pub fn wait_child(pid: Option<ProcessId>) -> Option<(ProcessId, i32)> {
    process::wait::wait_child(pid)
}

/// 等待子进程退出（返回详细状态）
pub fn wait_child_result(pid: Option<ProcessId>) -> WaitChildResult {
    process::wait::wait_child_result(pid)
}

/// 按目标等待子进程退出（返回详细状态）
pub fn wait_child_result_by_target(target: WaitTarget) -> WaitChildResult {
    process::wait::wait_child_result_by_target(target)
}

/// 仅观测子进程等待状态（不回收）
pub fn wait_child_observe_by_target(target: WaitTarget) -> WaitChildObserveResult {
    process::wait::observe_by_target(target)
}

/// 按目标+选项观测子进程等待状态
pub fn wait_child_observe_by_target_with_options(
    target: WaitTarget,
    options: WaitChildOptions,
) -> WaitChildObserveResult {
    process::wait::observe_by_target_with_options(target, options)
}

/// 回收已观测到的 Zombie 子进程
pub fn reap_observed_child(child_pid: ProcessId) -> Option<(ProcessId, i32)> {
    process::wait::reap_observed(child_pid)
}

/// 消费已观测到的等待事件（Stopped / Continued）
pub fn consume_observed_wait_event(child_pid: ProcessId, event: WaitChildConsumeEvent) -> bool {
    process::wait::consume_observed_event(child_pid, event)
}

/// 获取已观测子进程的 rusage 快照（仅父进程可见）
pub fn snapshot_observed_child_rusage(child_pid: ProcessId) -> Option<WaitRusageSnapshot> {
    process::wait::snapshot_observed_rusage(child_pid)
}

/// 按目标等待子进程事件（支持阻塞/非阻塞）
pub fn wait_event_by_target(
    target: WaitTarget,
    options: WaitChildOptions,
    nohang: bool,
) -> WaitEvent {
    process::wait::wait_event_by_target(target, options, nohang)
}
