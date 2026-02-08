//! x86_64 系统调用架构层（Linux ABI 编号）

mod table;

use crate::syscall::handlers;
use crate::syscall::{SyscallArch, SyscallArgs, SyscallDef, SyscallRet};

pub use table::SYSCALL_TABLE;

pub const NR_EXIT: usize = 60;
pub const NR_WAIT4: usize = 61;
pub const NR_GETPID: usize = 39;
pub const NR_GETPPID: usize = 110;
pub const NR_GETTID: usize = 186;
pub const NR_EXIT_GROUP: usize = 231;

pub struct X86_64SyscallArch;

pub static X86_64_SYSCALL_ARCH: X86_64SyscallArch = X86_64SyscallArch;

impl SyscallArch for X86_64SyscallArch {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet {
        match args.nr {
            NR_GETPID => handlers::sys_getpid(args),
            NR_GETPPID => handlers::sys_getppid(args),
            NR_GETTID => handlers::sys_gettid(args),
            NR_WAIT4 => handlers::sys_wait4(args),
            NR_EXIT => handlers::sys_exit(args),
            NR_EXIT_GROUP => handlers::sys_exit_group(args),
            _ => handlers::sys_ni(args),
        }
    }

    fn syscall_table(&self) -> &'static [SyscallDef] {
        SYSCALL_TABLE
    }
}
