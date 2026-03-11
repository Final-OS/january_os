use crate::errno::{E2BIG, ECHILD, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, EPERM, ESRCH};
use crate::fs;
use crate::syscall::{err, ok, SyscallArgs, SyscallRet};
use crate::task;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const WNOHANG: usize = 1;
const WUNTRACED: usize = 2;
const WCONTINUED: usize = 8;
const __WNOTHREAD: usize = 0x2000_0000;
const __WALL: usize = 0x4000_0000;
const __WCLONE: usize = 0x8000_0000;
const WAIT4_SUPPORTED_OPTS: usize =
    WNOHANG | WUNTRACED | WCONTINUED | __WNOTHREAD | __WALL | __WCLONE;

const MAX_SIGNAL: i32 = 64;

const EXEC_PATH_MAX: usize = 4096;
const EXEC_ARG_STR_MAX: usize = 4096;
const EXEC_ARG_LIST_MAX: usize = 256;
const EXEC_ENV_LIST_MAX: usize = 256;
const EXEC_TOTAL_BYTES_MAX: usize = 128 * 1024;

mod exec;
mod exec_args;
mod process;
mod signal;
mod wait;
mod wait_abi;

pub(crate) use exec::sys_execve;
pub(crate) use exec_args::*;
pub(crate) use process::{
    sys_clone, sys_exit, sys_exit_group, sys_fork, sys_getpgid, sys_getpgrp, sys_getpid,
    sys_getppid, sys_gettid, sys_kill, sys_setpgid, sys_setsid, sys_tgkill, sys_tkill, sys_vfork,
};
pub(crate) use signal::{sys_rt_sigaction, sys_rt_sigprocmask, sys_rt_sigreturn};
pub(crate) use wait::sys_wait4;
pub(crate) use wait_abi::*;
