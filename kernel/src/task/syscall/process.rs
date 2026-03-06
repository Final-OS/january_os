use super::*;

pub(crate) fn sys_getpid(_args: &SyscallArgs) -> SyscallRet {
    if let Some(pid) = task::current_pid() {
        ok(pid.0)
    } else {
        err(EINVAL)
    }
}

pub(crate) fn sys_getppid(_args: &SyscallArgs) -> SyscallRet {
    if let Some(ppid) = task::current_ppid() {
        ok(ppid.0)
    } else {
        ok(0)
    }
}

pub(crate) fn sys_gettid(_args: &SyscallArgs) -> SyscallRet {
    if let Some(tid) = task::current_tid() {
        ok(tid.0)
    } else {
        err(EINVAL)
    }
}

fn parse_signal(raw_sig: usize) -> Result<i32, i32> {
    let sig = raw_sig as i32;
    if sig < 0 || sig > MAX_SIGNAL {
        return Err(EINVAL);
    }
    Ok(sig)
}

pub(crate) fn sys_clone(args: &SyscallArgs) -> SyscallRet {
    match task::clone_current(args.arg0, args.arg1, args.arg2, args.arg3, args.arg4) {
        Ok(child_pid) => ok(child_pid.0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_fork(_args: &SyscallArgs) -> SyscallRet {
    match task::fork_current() {
        Ok(child_pid) => ok(child_pid.0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_vfork(_args: &SyscallArgs) -> SyscallRet {
    match task::vfork_current() {
        Ok(child_pid) => ok(child_pid.0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_getpgid(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    if raw_pid < 0 {
        return err(EINVAL);
    }

    let target_pid = if raw_pid == 0 {
        let Some(current_pid) = task::current_pid() else {
            return err(ESRCH);
        };
        current_pid
    } else {
        task::ProcessId(raw_pid as usize)
    };

    let Some(process) = task::find_process_by_pid(target_pid) else {
        return err(ESRCH);
    };

    ok(process.lock().pgid.0)
}

pub(crate) fn sys_getpgrp(_args: &SyscallArgs) -> SyscallRet {
    if let Some(pgid) = task::current_pgid() {
        ok(pgid.0)
    } else {
        err(ESRCH)
    }
}

pub(crate) fn sys_setpgid(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let raw_pgid = args.arg1 as isize;

    if raw_pid < 0 || raw_pgid < 0 {
        return err(EINVAL);
    }

    let Some(caller_pid) = task::current_pid() else {
        return err(ESRCH);
    };

    let target_pid = if raw_pid == 0 {
        caller_pid
    } else {
        task::ProcessId(raw_pid as usize)
    };

    let Some(target_process_ref) = task::find_process_by_pid(target_pid) else {
        return err(ESRCH);
    };

    if target_pid != caller_pid {
        let target_parent = target_process_ref.lock().parent;
        if target_parent != Some(caller_pid) {
            return err(EPERM);
        }
    }

    let new_pgid = if raw_pgid == 0 {
        target_pid
    } else {
        task::ProcessId(raw_pgid as usize)
    };

    if new_pgid != target_pid {
        let group_exists = task::find_process_by_pid(new_pgid).is_some()
            || !task::manager::process_ids_by_pgid(new_pgid).is_empty();
        if !group_exists {
            return err(EPERM);
        }
    }

    target_process_ref.lock().pgid = new_pgid;

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] setpgid target_pid={} new_pgid={} caller_pid={}",
            target_pid.0,
            new_pgid.0,
            caller_pid.0
        );
    }

    ok(0)
}

pub(crate) fn sys_setsid(_args: &SyscallArgs) -> SyscallRet {
    let Some(caller_pid) = task::current_pid() else {
        return err(ESRCH);
    };

    let Some(process_ref) = task::find_process_by_pid(caller_pid) else {
        return err(ESRCH);
    };

    let current_pgid = process_ref.lock().pgid;
    if current_pgid == caller_pid {
        return err(EPERM);
    }

    let conflicting_group = task::manager::process_ids_by_pgid(caller_pid)
        .into_iter()
        .any(|pid| pid != caller_pid);
    if conflicting_group {
        return err(EPERM);
    }

    process_ref.lock().pgid = caller_pid;

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] setsid pid={} pgid={} done",
            caller_pid.0,
            caller_pid.0
        );
    }

    ok(caller_pid.0)
}

pub(crate) fn sys_kill(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let sig = match parse_signal(args.arg1) {
        Ok(sig) => sig,
        Err(errno) => return err(errno),
    };

    let targets = match task::collect_kill_targets(raw_pid) {
        Ok(targets) => targets,
        Err(errno) => return err(errno),
    };

    let mut delivered = 0usize;
    let mut need_schedule = false;

    for pid in targets {
        match task::send_process_signal(pid, sig) {
            Ok(resched) => {
                delivered = delivered.saturating_add(1);
                need_schedule |= resched;
            }
            Err(ESRCH) => {}
            Err(errno) => return err(errno),
        }
    }

    if delivered == 0 {
        return err(ESRCH);
    }

    if need_schedule {
        task::scheduler::schedule();
    }

    ok(0)
}

pub(crate) fn sys_tkill(args: &SyscallArgs) -> SyscallRet {
    let raw_tid = args.arg0 as isize;
    if raw_tid <= 0 {
        return err(EINVAL);
    }

    let sig = match parse_signal(args.arg1) {
        Ok(sig) => sig,
        Err(errno) => return err(errno),
    };

    let Some(target_task) = task::manager::find_task_by_tid(task::TaskId(raw_tid as usize)) else {
        return err(ESRCH);
    };
    let target_pid = target_task.lock().pid;

    match task::send_process_signal(target_pid, sig) {
        Ok(need_schedule) => {
            if need_schedule {
                task::scheduler::schedule();
            }
            ok(0)
        }
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_tgkill(args: &SyscallArgs) -> SyscallRet {
    let raw_tgid = args.arg0 as isize;
    let raw_tid = args.arg1 as isize;

    if raw_tgid <= 0 || raw_tid <= 0 {
        return err(EINVAL);
    }

    let sig = match parse_signal(args.arg2) {
        Ok(sig) => sig,
        Err(errno) => return err(errno),
    };

    let Some(target_task) = task::manager::find_task_by_tid(task::TaskId(raw_tid as usize)) else {
        return err(ESRCH);
    };

    let target_pid = target_task.lock().pid;
    if target_pid.0 != raw_tgid as usize {
        return err(ESRCH);
    }

    match task::send_process_signal(target_pid, sig) {
        Ok(need_schedule) => {
            if need_schedule {
                task::scheduler::schedule();
            }
            ok(0)
        }
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_exit(args: &SyscallArgs) -> SyscallRet {
    let exit_code = args.arg0 as i32;
    task::exit_current_task(exit_code);
    task::scheduler::schedule();
    ok(0)
}

pub(crate) fn sys_exit_group(args: &SyscallArgs) -> SyscallRet {
    let exit_code = args.arg0 as i32;
    task::exit_current_process(exit_code);
    task::scheduler::schedule();
    ok(0)
}
