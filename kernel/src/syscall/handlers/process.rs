use crate::syscall::{ECHILD, EINVAL, SyscallArgs, SyscallRet, err, ok};
use crate::task;

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

pub(crate) fn sys_wait4(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let pid = if raw_pid < 0 {
        None
    } else {
        Some(task::TaskId(raw_pid as usize))
    };

    if let Some((reaped_pid, _)) = task::wait_child(pid) {
        ok(reaped_pid.0)
    } else {
        err(ECHILD)
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
