//! 系统调用抽象层

pub mod abi;
pub mod arch;
pub mod dispatch;
pub mod table;

use alloc::format;
use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub type SyscallRet = usize;
pub type SyscallHandler = fn(&SyscallArgs) -> SyscallRet;

pub use crate::errno::{
    E2BIG, EAGAIN, EBADF, EBUSY, ECHILD, EFAULT, EINVAL, EISDIR, ENAMETOOLONG, ENOENT, ENOMEM,
    ENOSYS, ENOTDIR, ENOTTY, EPERM, EPIPE, ERANGE, ESPIPE, ESRCH,
};

#[derive(Debug, Clone, Copy)]
pub struct SyscallArgs {
    pub nr: usize,
    pub arg0: usize,
    pub arg1: usize,
    pub arg2: usize,
    pub arg3: usize,
    pub arg4: usize,
    pub arg5: usize,
}

impl SyscallArgs {
    pub const fn new(
        nr: usize,
        arg0: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
        arg4: usize,
        arg5: usize,
    ) -> Self {
        Self {
            nr,
            arg0,
            arg1,
            arg2,
            arg3,
            arg4,
            arg5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SyscallDef {
    pub nr: usize,
    pub name: &'static str,
    pub domain: abi::SyscallDomain,
}

pub trait SyscallArch {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet;
    fn syscall_table(&self) -> &'static [SyscallDef];
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "syscall",
    stage: ComponentStage::Core,
    deps: &["interrupt", "task", "fs"],
    summary: "system call abi, table and subsystem dispatch",
};

#[inline]
pub(crate) fn ok(ret: usize) -> usize {
    ret
}

#[inline]
pub(crate) fn err(errno: i32) -> usize {
    (-(errno as isize)) as usize
}

pub fn dispatch(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> SyscallRet {
    let args = SyscallArgs::new(nr, arg0, arg1, arg2, arg3, arg4, arg5);
    dispatch::dispatch(&args)
}

pub fn syscall_table() -> &'static [SyscallDef] {
    arch::interface().syscall_table()
}

pub(crate) fn sys_ni(_args: &SyscallArgs) -> SyscallRet {
    err(ENOSYS)
}

pub fn init_early() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_core() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_late() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}

pub fn dump_state() -> String {
    format!(
        "component={} state={:?} table_entries={}",
        COMPONENT.id,
        stats().state,
        syscall_table().len(),
    )
}
