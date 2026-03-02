//! 系统调用抽象层

pub mod arch;
pub(crate) mod handlers;

pub type SyscallRet = usize;
pub type SyscallHandler = fn(&SyscallArgs) -> SyscallRet;

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ESRCH: i32 = 3;
pub const E2BIG: i32 = 7;
pub const EBADF: i32 = 9;
pub const EAGAIN: i32 = 11;
pub const ECHILD: i32 = 10;
pub const ENOMEM: i32 = 12;
pub const EFAULT: i32 = 14;
pub const EBUSY: i32 = 16;
pub const EINVAL: i32 = 22;
pub const ENOTTY: i32 = 25;
pub const EPIPE: i32 = 32;
pub const ENAMETOOLONG: i32 = 36;
pub const ENOSYS: i32 = 38;

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
}

pub trait SyscallArch {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet;
    fn syscall_table(&self) -> &'static [SyscallDef];
}

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
    arch::interface().dispatch(&args)
}

pub fn syscall_table() -> &'static [SyscallDef] {
    arch::interface().syscall_table()
}
