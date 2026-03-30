pub mod api;
pub mod arch;
pub mod diag;
pub mod ipc;
pub mod proc;
pub mod runtime;
pub mod sched;
pub mod syscall;
pub mod thread;

use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub use api::{Context, Pid, ProcessId, TaskContext, TaskId};
pub use proc::exec::{
    ExecLoadPlan, ExecMapPreview, ExecMappedPage, ExecMappedPageKind, ExecVmaRestorePoint,
    build_elf_load_plan, install_current_exec_vmas, preview_pt_load_mapping, remap_exec_mappings,
    restore_current_exec_vmas, rollback_exec_mappings, setup_initial_user_stack,
    stage_pt_load_mappings, unmap_exec_mappings,
};
pub use proc::fork::{clone_current, fork_current, vfork_current};
pub use proc::signal::{collect_kill_targets, send_signal};
pub use proc::wait::WaitEvent;
pub use runtime::manager::{
    SpawnMmMode, WaitChildConsumeEvent, WaitChildObserveResult, WaitChildOptions, WaitChildResult,
    WaitCloneFilter, WaitRusageSnapshot, WaitTarget, current_mm_ptr, find_process_by_pid,
    find_task_by_pid, find_task_by_tid, lookup_current_exec_mapping, record_current_exec_request,
    set_current_exec_mappings, spawn_kernel_thread, spawn_kernel_thread_with_mm_mode,
    spawn_kernel_thread_with_mm_mode_checked, take_current_exec_mappings,
};
pub use sched::SchedulerStats;
pub use sched::snapshot_stats as scheduler_snapshot_stats;
pub use thread::{Task, TaskStatus, current_task};

#[derive(Debug, Clone, Copy)]
pub struct TaskInitReport {
    pub scheduler_ready: bool,
    pub process_runtime_ready: bool,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "task",
    stage: ComponentStage::Late,
    deps: &["fs", "timer", "memory"],
    summary: "task, process, scheduler, wait and signal runtime",
};

pub fn init_runtime() -> TaskInitReport {
    crate::info!("[TASK] Initializing Task subsystem...");
    runtime::init::init();
    crate::ok!("[TASK] Task subsystem initialized.");

    TaskInitReport {
        scheduler_ready: true,
        process_runtime_ready: true,
    }
}

pub fn init() {
    let _ = init_runtime();
}

pub fn init_early() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_core() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_late() -> crate::error::KernelResult<()> {
    let _ = init_runtime();
    Ok(())
}

pub fn stats() -> ComponentStats {
    diag::stats::component_stats()
}

pub fn dump_state() -> String {
    diag::dump::dump_state()
}

pub fn current_pid() -> Option<ProcessId> {
    current_task().map(|task| task.lock().pid)
}

pub fn current_ppid() -> Option<ProcessId> {
    current_task().map(|task| task.lock().ppid)
}

pub fn current_pgid() -> Option<ProcessId> {
    let pid = current_pid()?;
    find_process_by_pid(pid).map(|process| process.lock().pgid)
}

pub fn current_tid() -> Option<TaskId> {
    current_task().map(|task| task.lock().id)
}

pub fn exit_current_task(exit_code: i32) {
    proc::exit::exit_current_task(exit_code);
}

pub fn exit_current_process(exit_code: i32) {
    proc::exit::exit_current_process(exit_code);
}

pub fn wait_child(pid: Option<ProcessId>) -> Option<(ProcessId, i32)> {
    proc::wait::wait_child(pid)
}

pub fn wait_child_result(pid: Option<ProcessId>) -> WaitChildResult {
    proc::wait::wait_child_result(pid)
}

pub fn wait_child_result_by_target(target: WaitTarget) -> WaitChildResult {
    proc::wait::wait_child_result_by_target(target)
}

pub fn wait_child_observe_by_target(target: WaitTarget) -> WaitChildObserveResult {
    proc::wait::observe_by_target(target)
}

pub fn wait_child_observe_by_target_with_options(
    target: WaitTarget,
    options: WaitChildOptions,
) -> WaitChildObserveResult {
    proc::wait::observe_by_target_with_options(target, options)
}

pub fn reap_observed_child(child_pid: ProcessId) -> Option<(ProcessId, i32)> {
    proc::wait::reap_observed(child_pid)
}

pub fn consume_observed_wait_event(child_pid: ProcessId, event: WaitChildConsumeEvent) -> bool {
    proc::wait::consume_observed_event(child_pid, event)
}

pub fn snapshot_observed_child_rusage(child_pid: ProcessId) -> Option<WaitRusageSnapshot> {
    proc::wait::snapshot_observed_rusage(child_pid)
}

pub fn wait_event_by_target(
    target: WaitTarget,
    options: WaitChildOptions,
    nohang: bool,
) -> WaitEvent {
    proc::wait::wait_event_by_target(target, options, nohang)
}

pub fn send_process_signal(pid: ProcessId, sig: i32) -> Result<bool, i32> {
    proc::signal::send_signal(pid, sig)
}
