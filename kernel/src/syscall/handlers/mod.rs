//! 系统调用通用实现

mod common;
mod file;
mod memory;
mod process;
mod signal;

pub(crate) use common::sys_ni;
pub(crate) use file::{
    sys_close,
    sys_fstat,
    sys_ioctl,
    sys_lstat,
    sys_open,
    sys_pipe,
    sys_pipe2,
    sys_poll,
    sys_read,
    sys_select,
    sys_stat,
    sys_write,
};
pub(crate) use memory::{sys_brk, sys_mmap, sys_mprotect, sys_munmap};
pub(crate) use process::{
    sys_clone,
    sys_execve,
    sys_exit,
    sys_exit_group,
    sys_fork,
    sys_getpgid,
    sys_getpgrp,
    sys_getpid,
    sys_getppid,
    sys_gettid,
    sys_kill,
    sys_setpgid,
    sys_setsid,
    sys_tgkill,
    sys_tkill,
    sys_vfork,
    sys_wait4,
};
pub(crate) use signal::{sys_rt_sigaction, sys_rt_sigprocmask, sys_rt_sigreturn};
