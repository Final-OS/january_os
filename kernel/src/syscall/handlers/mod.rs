//! 系统调用通用实现

mod common;
mod process;

pub(crate) use common::sys_ni;
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
