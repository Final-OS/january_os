use alloc::{vec, vec::Vec};

use crate::errno::{EINVAL, ESRCH};
use crate::task;

const SIGCHLD: i32 = 17;
const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;
const SIGSTOP: i32 = 19;
const SIGCONT: i32 = 18;

pub fn collect_kill_targets(raw_pid: isize) -> Result<Vec<task::ProcessId>, i32> {
    match raw_pid {
        pid if pid > 0 => {
            let target = task::ProcessId(pid as usize);
            if task::find_process_by_pid(target).is_some() {
                Ok(vec![target])
            } else {
                Err(ESRCH)
            }
        }
        0 => {
            let Some(current_pgid) = task::current_pgid() else {
                return Err(ESRCH);
            };
            let targets = task::runtime::manager::process_ids_by_pgid(current_pgid);
            if targets.is_empty() {
                Err(ESRCH)
            } else {
                Ok(targets)
            }
        }
        -1 => {
            let mut targets: Vec<task::ProcessId> = task::runtime::manager::all_process_ids()
                .into_iter()
                .filter(|pid| pid.0 != 0)
                .collect();
            targets.sort_by_key(|pid| pid.0);
            if targets.is_empty() {
                Err(ESRCH)
            } else {
                Ok(targets)
            }
        }
        pid => {
            let group_raw = pid.checked_neg().ok_or(EINVAL)?;
            if group_raw <= 1 {
                return Err(EINVAL);
            }
            let group = task::ProcessId(group_raw as usize);
            let targets = task::runtime::manager::process_ids_by_pgid(group);
            if targets.is_empty() {
                Err(ESRCH)
            } else {
                Ok(targets)
            }
        }
    }
}

pub fn send_signal(pid: task::ProcessId, sig: i32) -> Result<bool, i32> {
    let Some(process_ref) = task::find_process_by_pid(pid) else {
        return Err(ESRCH);
    };

    if sig == 0 || sig == SIGCHLD {
        return Ok(false);
    }

    let current_pid = task::current_pid();
    let target_is_current = current_pid == Some(pid);

    match sig {
        SIGTERM | SIGKILL => {
            let exit_code = 128 + sig;
            let tasks = {
                let process = process_ref.lock();
                process.tasks.clone()
            };

            for task_ref in tasks.iter() {
                let mut task = task_ref.lock();
                task.status = task::TaskStatus::Exited;
                task.exit_code = Some(exit_code);
            }

            {
                let mut process = process_ref.lock();
                process.mark_exiting(exit_code);
                process.mark_zombie();
            }

            let removed_ready = task::sched::SCHEDULER.remove_tasks_by_pid(pid);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[signal] terminate pid={} sig={} removed_ready={}",
                    pid.0,
                    sig,
                    removed_ready
                );
            }

            Ok(target_is_current)
        }
        SIGSTOP => {
            let tasks = {
                let mut process = process_ref.lock();
                process.mark_stopped(SIGSTOP);
                process.tasks.clone()
            };

            let mut blocked_tasks = 0usize;
            for task_ref in tasks {
                let mut task = task_ref.lock();
                if task.status != task::TaskStatus::Exited {
                    task.status = task::TaskStatus::Blocked;
                    blocked_tasks = blocked_tasks.saturating_add(1);
                }
            }

            let removed_ready = task::sched::SCHEDULER.remove_tasks_by_pid(pid);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[signal] stop pid={} blocked_tasks={} removed_ready={}",
                    pid.0,
                    blocked_tasks,
                    removed_ready
                );
            }

            Ok(target_is_current)
        }
        SIGCONT => {
            let tasks = {
                let mut process = process_ref.lock();
                process.mark_continued();
                process.tasks.clone()
            };

            let mut resumed_tasks = 0usize;
            for task_ref in tasks {
                let mut should_queue = false;
                {
                    let mut task = task_ref.lock();
                    if task.status == task::TaskStatus::Blocked {
                        task.status = task::TaskStatus::Ready;
                        should_queue = true;
                    }
                }

                if should_queue {
                    task::sched::SCHEDULER.add_task(task_ref);
                    resumed_tasks = resumed_tasks.saturating_add(1);
                }
            }

            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[signal] continue pid={} resumed_tasks={}",
                    pid.0,
                    resumed_tasks
                );
            }
            Ok(false)
        }
        _ => Err(EINVAL),
    }
}
