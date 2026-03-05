use crate::syscall::{ENOSYS, SyscallArch, SyscallArgs, SyscallDef, SyscallRet};

pub struct Aarch64SyscallArch;

impl SyscallArch for Aarch64SyscallArch {
    fn dispatch(&self, _args: &SyscallArgs) -> SyscallRet {
        (-(ENOSYS as isize)) as usize
    }

    fn syscall_table(&self) -> &'static [SyscallDef] {
        &[]
    }
}

pub static AARCH64_SYSCALL_ARCH: Aarch64SyscallArch = Aarch64SyscallArch;
