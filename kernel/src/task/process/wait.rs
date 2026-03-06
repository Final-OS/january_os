use crate::task;

#[inline]
pub fn observe_by_target_with_options(
    target: task::WaitTarget,
    options: task::WaitChildOptions,
) -> task::WaitChildObserveResult {
    task::manager::wait_child_observe_by_target_with_options(target, options)
}

#[inline]
pub fn observe_by_target(target: task::WaitTarget) -> task::WaitChildObserveResult {
    task::manager::wait_child_observe_by_target(target)
}

#[inline]
pub fn reap_observed(child_pid: task::ProcessId) -> Option<(task::ProcessId, i32)> {
    task::manager::reap_observed_child(child_pid)
}

#[inline]
pub fn consume_observed_event(
    child_pid: task::ProcessId,
    event: task::WaitChildConsumeEvent,
) -> bool {
    task::manager::consume_observed_wait_event(child_pid, event)
}

#[inline]
pub fn wait_child_result_by_target(target: task::WaitTarget) -> task::WaitChildResult {
    task::manager::wait_child_result_by_target(target)
}

#[inline]
pub fn wait_child_result(target_pid: Option<task::ProcessId>) -> task::WaitChildResult {
    task::manager::wait_child_result(target_pid)
}

#[inline]
pub fn wait_child(target_pid: Option<task::ProcessId>) -> Option<(task::ProcessId, i32)> {
    task::manager::wait_child(target_pid)
}

#[inline]
pub fn snapshot_observed_rusage(child_pid: task::ProcessId) -> Option<task::WaitRusageSnapshot> {
    task::manager::snapshot_observed_child_rusage(child_pid)
}
