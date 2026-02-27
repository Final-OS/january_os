//! x86_64 系统调用架构层（Linux ABI 编号）

mod table;

use crate::syscall::handlers;
use crate::syscall::{SyscallArch, SyscallArgs, SyscallDef, SyscallRet};

pub use table::SYSCALL_TABLE;

pub const NR_READ: usize = 0;
pub const NR_OPEN: usize = 2;
pub const NR_CLOSE: usize = 3;
pub const NR_MMAP: usize = 9;
pub const NR_MUNMAP: usize = 11;
pub const NR_GETPID: usize = 39;
pub const NR_CLONE: usize = 56;
pub const NR_FORK: usize = 57;
pub const NR_VFORK: usize = 58;
pub const NR_EXECVE: usize = 59;
pub const NR_EXIT: usize = 60;
pub const NR_WAIT4: usize = 61;
pub const NR_KILL: usize = 62;
pub const NR_SETPGID: usize = 109;
pub const NR_GETPPID: usize = 110;
pub const NR_GETPGRP: usize = 111;
pub const NR_SETSID: usize = 112;
pub const NR_GETPGID: usize = 121;
pub const NR_GETTID: usize = 186;
pub const NR_TKILL: usize = 200;
pub const NR_EXIT_GROUP: usize = 231;
pub const NR_TGKILL: usize = 234;

pub struct X86_64SyscallArch;

pub static X86_64_SYSCALL_ARCH: X86_64SyscallArch = X86_64SyscallArch;

impl SyscallArch for X86_64SyscallArch {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet {
        match args.nr {
            NR_READ => handlers::sys_read(args),
            NR_OPEN => handlers::sys_open(args),
            NR_CLOSE => handlers::sys_close(args),
            NR_MMAP => handlers::sys_mmap(args),
            NR_MUNMAP => handlers::sys_munmap(args),
            NR_GETPID => handlers::sys_getpid(args),
            NR_CLONE => handlers::sys_clone(args),
            NR_FORK => handlers::sys_fork(args),
            NR_VFORK => handlers::sys_vfork(args),
            NR_EXECVE => handlers::sys_execve(args),
            NR_EXIT => handlers::sys_exit(args),
            NR_WAIT4 => handlers::sys_wait4(args),
            NR_KILL => handlers::sys_kill(args),
            NR_SETPGID => handlers::sys_setpgid(args),
            NR_GETPPID => handlers::sys_getppid(args),
            NR_GETPGRP => handlers::sys_getpgrp(args),
            NR_SETSID => handlers::sys_setsid(args),
            NR_GETPGID => handlers::sys_getpgid(args),
            NR_GETTID => handlers::sys_gettid(args),
            NR_TKILL => handlers::sys_tkill(args),
            NR_EXIT_GROUP => handlers::sys_exit_group(args),
            NR_TGKILL => handlers::sys_tgkill(args),
            _ => handlers::sys_ni(args),
        }
    }

    fn syscall_table(&self) -> &'static [SyscallDef] {
        SYSCALL_TABLE
    }
}
