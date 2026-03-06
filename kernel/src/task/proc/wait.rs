use crate::task;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitEvent {
    Exited {
        pid: task::ProcessId,
        exit_code: i32,
        rusage: task::WaitRusageSnapshot,
    },
    Stopped {
        pid: task::ProcessId,
        signal: i32,
        rusage: task::WaitRusageSnapshot,
    },
    Continued {
        pid: task::ProcessId,
        rusage: task::WaitRusageSnapshot,
    },
    NoMatchedChild,
    StillRunning,
}

#[inline]
pub fn observe_by_target_with_options(
    target: task::WaitTarget,
    options: task::WaitChildOptions,
) -> task::WaitChildObserveResult {
    task::runtime::manager::wait_child_observe_by_target_with_options(target, options)
}

#[inline]
pub fn observe_by_target(target: task::WaitTarget) -> task::WaitChildObserveResult {
    task::runtime::manager::wait_child_observe_by_target(target)
}

#[inline]
pub fn reap_observed(child_pid: task::ProcessId) -> Option<(task::ProcessId, i32)> {
    task::runtime::manager::reap_observed_child(child_pid)
}

#[inline]
pub fn consume_observed_event(
    child_pid: task::ProcessId,
    event: task::WaitChildConsumeEvent,
) -> bool {
    task::runtime::manager::consume_observed_wait_event(child_pid, event)
}

#[inline]
pub fn wait_child_result_by_target(target: task::WaitTarget) -> task::WaitChildResult {
    task::runtime::manager::wait_child_result_by_target(target)
}

#[inline]
pub fn wait_child_result(target_pid: Option<task::ProcessId>) -> task::WaitChildResult {
    task::runtime::manager::wait_child_result(target_pid)
}

#[inline]
pub fn wait_child(target_pid: Option<task::ProcessId>) -> Option<(task::ProcessId, i32)> {
    task::runtime::manager::wait_child(target_pid)
}

#[inline]
pub fn snapshot_observed_rusage(child_pid: task::ProcessId) -> Option<task::WaitRusageSnapshot> {
    task::runtime::manager::snapshot_observed_child_rusage(child_pid)
}

pub fn wait_event_by_target(
    target: task::WaitTarget,
    options: task::WaitChildOptions,
    nohang: bool,
) -> WaitEvent {
    let mut logged_waiting = false;

    loop {
        match observe_by_target_with_options(target, options) {
            task::WaitChildObserveResult::Reapable(child_pid, _exit_code) => {
                let rusage = snapshot_observed_rusage(child_pid).unwrap_or_default();
                let Some((reaped_pid, reaped_exit_code)) = reap_observed(child_pid) else {
                    continue;
                };

                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[task] wait reap child pid={} target={:?}",
                        reaped_pid.0,
                        target
                    );
                }

                return WaitEvent::Exited {
                    pid: reaped_pid,
                    exit_code: reaped_exit_code,
                    rusage,
                };
            }
            task::WaitChildObserveResult::Stopped(child_pid, signal) => {
                let rusage = snapshot_observed_rusage(child_pid).unwrap_or_default();
                if !consume_observed_event(child_pid, task::WaitChildConsumeEvent::Stopped) {
                    continue;
                }

                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[task] wait stopped child pid={} sig={} target={:?}",
                        child_pid.0,
                        signal,
                        target
                    );
                }

                return WaitEvent::Stopped {
                    pid: child_pid,
                    signal,
                    rusage,
                };
            }
            task::WaitChildObserveResult::Continued(child_pid) => {
                let rusage = snapshot_observed_rusage(child_pid).unwrap_or_default();
                if !consume_observed_event(child_pid, task::WaitChildConsumeEvent::Continued) {
                    continue;
                }

                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[task] wait continued child pid={} target={:?}",
                        child_pid.0,
                        target
                    );
                }

                return WaitEvent::Continued {
                    pid: child_pid,
                    rusage,
                };
            }
            task::WaitChildObserveResult::NoMatchedChild => return WaitEvent::NoMatchedChild,
            task::WaitChildObserveResult::ChildRunning => {
                if nohang {
                    return WaitEvent::StillRunning;
                }

                if !logged_waiting {
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[task] wait blocking target={:?}",
                            target
                        );
                    }
                    logged_waiting = true;
                }

                task::sched::schedule();
            }
        }
    }
}
