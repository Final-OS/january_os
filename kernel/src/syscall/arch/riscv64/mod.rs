use crate::errno::ENOSYS;
use crate::syscall::{SyscallArch, SyscallArgs, SyscallDef, SyscallRet};

pub struct Riscv64SyscallArch;

impl SyscallArch for Riscv64SyscallArch {
    fn dispatch(&self, _args: &SyscallArgs) -> SyscallRet {
        (-(ENOSYS as isize)) as usize
    }

    fn syscall_table(&self) -> &'static [SyscallDef] {
        &[]
    }
}

pub static RISCV64_SYSCALL_ARCH: Riscv64SyscallArch = Riscv64SyscallArch;
