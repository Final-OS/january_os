use alloc::sync::Arc;

use crate::errno::{EINVAL, ENOMEM, ESRCH};
use crate::sync::Mutex;
use crate::task;
use crate::task::thread::Task;

const SIGCHLD: i32 = 17;
const MAX_SIGNAL: i32 = 64;

const CSIGNAL: usize = 0x0000_00ff;
const CLONE_VM: usize = 0x0000_0100;
const CLONE_VFORK: usize = 0x0000_4000;
const CLONE_THREAD: usize = 0x0001_0000;
const CLONE_PARENT_SETTID: usize = 0x0010_0000;
const CLONE_CHILD_CLEARTID: usize = 0x0020_0000;
const CLONE_CHILD_SETTID: usize = 0x0100_0000;
const CLONE_SETTLS: usize = 0x0008_0000;
const CLONE_DETACHED: usize = 0x0040_0000;
const CLONE_SUPPORTED_FLAGS: usize = CSIGNAL | CLONE_VM | CLONE_VFORK;

#[inline(never)]
fn fail_and_exit_current_task(code: i32) -> ! {
    super::exit::exit_current_task(code);
    loop {
        task::sched::schedule();
    }
}

extern "C" fn syscall_child_return_stub() {
    let Some(task_ref) = task::current_task() else {
        fail_and_exit_current_task(127);
    };

    let frame = {
        let mut task = task_ref.lock();
        task.fork_return_frame.take()
    };
    let Some(frame) = frame else {
        fail_and_exit_current_task(127);
    };

    unsafe {
        task::arch::enter_user_fork_return(&frame);
    }
}

fn spawn_minimal_child(
    name: &str,
    is_clone_child: bool,
    mm_mode: task::SpawnMmMode,
    fork_frame: task::arch::ForkReturnFrame,
) -> Result<task::ProcessId, i32> {
    let child_task =
        task::spawn_kernel_thread_with_mm_mode_checked(name, syscall_child_return_stub, mm_mode)
            .ok_or(ENOMEM)?;
    {
        let mut child = child_task.lock();
        child.fork_return_frame = Some(fork_frame);
    }
    let child_pid = child_task.lock().pid;

    let Some(child_process) = task::find_process_by_pid(child_pid) else {
        return Err(ESRCH);
    };

    child_process.lock().is_clone_child = is_clone_child;

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] syscall spawn child: pid={} clone_child={} name={}",
            child_pid.0,
            is_clone_child,
            name
        );
    }

    Ok(child_pid)
}

fn wait_for_vfork_release(child_pid: task::ProcessId) {
    let options = task::WaitChildOptions {
        clone_filter: task::WaitCloneFilter::All,
        ..task::WaitChildOptions::default()
    };

    let mut logged_wait = false;
    loop {
        match task::wait_child_observe_by_target_with_options(
            task::WaitTarget::Pid(child_pid),
            options,
        ) {
            task::WaitChildObserveResult::Reapable(_, _)
            | task::WaitChildObserveResult::NoMatchedChild => {
                break;
            }
            task::WaitChildObserveResult::Stopped(_, _)
            | task::WaitChildObserveResult::Continued(_)
            | task::WaitChildObserveResult::ChildRunning => {
                if !logged_wait {
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[task] vfork parent waiting child_pid={}",
                            child_pid.0
                        );
                    }
                    logged_wait = true;
                }
                task::sched::schedule();
            }
        }
    }
}

pub fn clone_current(
    flags: usize,
    child_stack: usize,
    parent_tid: usize,
    child_tid: usize,
    tls: usize,
) -> Result<task::ProcessId, i32> {
    let unsupported = flags & !CLONE_SUPPORTED_FLAGS;
    if unsupported != 0 {
        return Err(EINVAL);
    }

    if (flags & CLONE_VFORK) != 0 && (flags & CLONE_VM) == 0 {
        return Err(EINVAL);
    }

    if (flags & CLONE_VM) != 0 && (flags & CLONE_VFORK) == 0 {
        return Err(EINVAL);
    }

    if child_stack != 0
        || parent_tid != 0
        || child_tid != 0
        || tls != 0
        || (flags & CLONE_THREAD) != 0
        || (flags & CLONE_PARENT_SETTID) != 0
        || (flags & CLONE_CHILD_SETTID) != 0
        || (flags & CLONE_CHILD_CLEARTID) != 0
        || (flags & CLONE_SETTLS) != 0
        || (flags & CLONE_DETACHED) != 0
    {
        return Err(EINVAL);
    }

    let exit_signal = (flags & CSIGNAL) as i32;
    if exit_signal < 0 || exit_signal > MAX_SIGNAL {
        return Err(EINVAL);
    }

    let is_clone_child = exit_signal != 0 && exit_signal != SIGCHLD;
    let child_name = if (flags & CLONE_VFORK) != 0 {
        "vfork_child"
    } else if is_clone_child {
        "clone_child"
    } else {
        "fork_child"
    };

    let mm_mode = if (flags & CLONE_VM) != 0 {
        task::SpawnMmMode::InheritShared
    } else {
        task::SpawnMmMode::InheritPrivate
    };

    let fork_frame = crate::arch::syscall::current_fork_return_frame().ok_or(EINVAL)?;
    let child_pid = spawn_minimal_child(child_name, is_clone_child, mm_mode, fork_frame)?;

    if (flags & CLONE_VFORK) != 0 {
        wait_for_vfork_release(child_pid);
    }

    Ok(child_pid)
}

#[inline]
pub fn fork_current() -> Result<task::ProcessId, i32> {
    clone_current(SIGCHLD as usize, 0, 0, 0, 0)
}

#[inline]
pub fn vfork_current() -> Result<task::ProcessId, i32> {
    clone_current((SIGCHLD as usize) | CLONE_VM | CLONE_VFORK, 0, 0, 0, 0)
}
