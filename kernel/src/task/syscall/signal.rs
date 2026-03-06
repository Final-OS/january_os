use alloc::collections::BTreeMap;

use crate::errno::{EINVAL, ENOSYS, ESRCH};
use crate::common::uaccess::{read_user_struct, read_user_u64, write_user_struct, write_user_u64};
use crate::sync::Mutex;
use crate::syscall::{err, ok, SyscallArgs, SyscallRet};
use crate::task;

const MAX_SIGNAL: usize = 64;
const SIGKILL: usize = 9;
const SIGSTOP: usize = 19;
const RT_SIGSET_SIZE: usize = core::mem::size_of::<u64>();

const SIG_BLOCK: usize = 0;
const SIG_UNBLOCK: usize = 1;
const SIG_SETMASK: usize = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RtSigAction {
    handler: usize,
    flags: u64,
    restorer: usize,
    mask: u64,
}

static SIGNAL_ACTIONS: Mutex<BTreeMap<usize, [RtSigAction; MAX_SIGNAL + 1]>> =
    Mutex::new(BTreeMap::new());
static SIGNAL_MASKS: Mutex<BTreeMap<usize, u64>> = Mutex::new(BTreeMap::new());

#[inline]
fn current_pid_raw() -> Result<usize, i32> {
    task::current_pid().map(|pid| pid.0).ok_or(ESRCH)
}

#[inline]
fn validate_sig(sig: usize) -> Result<usize, i32> {
    if sig == 0 || sig > MAX_SIGNAL {
        return Err(EINVAL);
    }
    Ok(sig)
}

#[inline]
fn read_user_sigaction(ptr: usize) -> Result<RtSigAction, i32> {
    read_user_struct(ptr)
}

#[inline]
fn write_user_sigaction(ptr: usize, value: RtSigAction) -> Result<(), i32> {
    write_user_struct(ptr, &value)
}

#[inline]
fn non_blockable_mask() -> u64 {
    (1u64 << (SIGKILL - 1)) | (1u64 << (SIGSTOP - 1))
}

pub(crate) fn sys_rt_sigaction(args: &SyscallArgs) -> SyscallRet {
    let sig = match validate_sig(args.arg0) {
        Ok(v) => v,
        Err(errno) => return err(errno),
    };
    let act_ptr = args.arg1;
    let oldact_ptr = args.arg2;
    let sigsetsize = args.arg3;
    if sigsetsize != RT_SIGSET_SIZE {
        return err(EINVAL);
    }
    if (sig == SIGKILL || sig == SIGSTOP) && act_ptr != 0 {
        return err(EINVAL);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let mut actions = SIGNAL_ACTIONS.lock();
    let table = actions
        .entry(pid)
        .or_insert_with(|| [RtSigAction::default(); MAX_SIGNAL + 1]);

    if oldact_ptr != 0 {
        if let Err(errno) = write_user_sigaction(oldact_ptr, table[sig]) {
            return err(errno);
        }
    }

    if act_ptr != 0 {
        let new_action = match read_user_sigaction(act_ptr) {
            Ok(v) => v,
            Err(errno) => return err(errno),
        };
        table[sig] = new_action;
    }

    ok(0)
}

pub(crate) fn sys_rt_sigprocmask(args: &SyscallArgs) -> SyscallRet {
    let how = args.arg0;
    let set_ptr = args.arg1;
    let oldset_ptr = args.arg2;
    let sigsetsize = args.arg3;
    if sigsetsize != RT_SIGSET_SIZE {
        return err(EINVAL);
    }
    if how != SIG_BLOCK && how != SIG_UNBLOCK && how != SIG_SETMASK {
        return err(EINVAL);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let mut masks = SIGNAL_MASKS.lock();
    let current = *masks.get(&pid).unwrap_or(&0);

    if oldset_ptr != 0 {
        if let Err(errno) = write_user_u64(oldset_ptr, current) {
            return err(errno);
        }
    }

    if set_ptr != 0 {
        let mut new_mask = match read_user_u64(set_ptr) {
            Ok(v) => v,
            Err(errno) => return err(errno),
        };
        new_mask &= !non_blockable_mask();

        let updated = match how {
            SIG_BLOCK => current | new_mask,
            SIG_UNBLOCK => current & !new_mask,
            SIG_SETMASK => new_mask,
            _ => current,
        };
        masks.insert(pid, updated);
    }

    ok(0)
}

pub(crate) fn sys_rt_sigreturn(_args: &SyscallArgs) -> SyscallRet {
    err(ENOSYS)
}
