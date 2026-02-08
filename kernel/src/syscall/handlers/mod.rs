//! 系统调用通用实现

mod common;
mod process;

pub(crate) use common::sys_ni;
pub(crate) use process::{sys_exit, sys_exit_group, sys_getpid, sys_getppid, sys_gettid, sys_wait4};
