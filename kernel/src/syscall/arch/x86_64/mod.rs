//! x86_64 系统调用架构层（Linux ABI 编号）

mod table;

use crate::syscall::{SyscallArch, SyscallArgs, SyscallDef, SyscallRet};
use crate::{fs, mm, task};

pub use table::SYSCALL_TABLE;

pub const NR_READ: usize = 0;
pub const NR_WRITE: usize = 1;
pub const NR_OPEN: usize = 2;
pub const NR_CLOSE: usize = 3;
pub const NR_STAT: usize = 4;
pub const NR_FSTAT: usize = 5;
pub const NR_LSTAT: usize = 6;
pub const NR_POLL: usize = 7;
pub const NR_LSEEK: usize = 8;
pub const NR_MMAP: usize = 9;
pub const NR_MPROTECT: usize = 10;
pub const NR_MUNMAP: usize = 11;
pub const NR_BRK: usize = 12;
pub const NR_RT_SIGACTION: usize = 13;
pub const NR_RT_SIGPROCMASK: usize = 14;
pub const NR_RT_SIGRETURN: usize = 15;
pub const NR_IOCTL: usize = 16;
pub const NR_ACCESS: usize = 21;
pub const NR_PIPE: usize = 22;
pub const NR_SELECT: usize = 23;
pub const NR_DUP: usize = 32;
pub const NR_DUP2: usize = 33;
pub const NR_GETPID: usize = 39;
pub const NR_CLONE: usize = 56;
pub const NR_FORK: usize = 57;
pub const NR_VFORK: usize = 58;
pub const NR_EXECVE: usize = 59;
pub const NR_EXIT: usize = 60;
pub const NR_WAIT4: usize = 61;
pub const NR_KILL: usize = 62;
pub const NR_FCNTL: usize = 72;
pub const NR_GETCWD: usize = 79;
pub const NR_CHDIR: usize = 80;
pub const NR_FCHDIR: usize = 81;
pub const NR_STATFS: usize = 137;
pub const NR_FSTATFS: usize = 138;
pub const NR_SETPGID: usize = 109;
pub const NR_GETPPID: usize = 110;
pub const NR_GETPGRP: usize = 111;
pub const NR_SETSID: usize = 112;
pub const NR_GETPGID: usize = 121;
pub const NR_GETTID: usize = 186;
pub const NR_GETDENTS64: usize = 217;
pub const NR_TKILL: usize = 200;
pub const NR_EXIT_GROUP: usize = 231;
pub const NR_TGKILL: usize = 234;
pub const NR_DUP3: usize = 292;
pub const NR_PIPE2: usize = 293;

pub struct X86_64SyscallArch;

pub static X86_64_SYSCALL_ARCH: X86_64SyscallArch = X86_64SyscallArch;

impl SyscallArch for X86_64SyscallArch {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet {
        match args.nr {
            NR_READ => fs::syscall::sys_read(args),
            NR_WRITE => fs::syscall::sys_write(args),
            NR_OPEN => fs::syscall::sys_open(args),
            NR_CLOSE => fs::syscall::sys_close(args),
            NR_STAT => fs::syscall::sys_stat(args),
            NR_FSTAT => fs::syscall::sys_fstat(args),
            NR_LSTAT => fs::syscall::sys_lstat(args),
            NR_POLL => fs::syscall::sys_poll(args),
            NR_LSEEK => fs::syscall::sys_lseek(args),
            NR_MMAP => mm::syscall::sys_mmap(args),
            NR_MPROTECT => mm::syscall::sys_mprotect(args),
            NR_MUNMAP => mm::syscall::sys_munmap(args),
            NR_BRK => mm::syscall::sys_brk(args),
            NR_RT_SIGACTION => task::syscall::sys_rt_sigaction(args),
            NR_RT_SIGPROCMASK => task::syscall::sys_rt_sigprocmask(args),
            NR_RT_SIGRETURN => task::syscall::sys_rt_sigreturn(args),
            NR_IOCTL => fs::syscall::sys_ioctl(args),
            NR_ACCESS => fs::syscall::sys_access(args),
            NR_PIPE => fs::syscall::sys_pipe(args),
            NR_SELECT => fs::syscall::sys_select(args),
            NR_DUP => fs::syscall::sys_dup(args),
            NR_DUP2 => fs::syscall::sys_dup2(args),
            NR_GETPID => task::syscall::sys_getpid(args),
            NR_CLONE => task::syscall::sys_clone(args),
            NR_FORK => task::syscall::sys_fork(args),
            NR_VFORK => task::syscall::sys_vfork(args),
            NR_EXECVE => task::syscall::sys_execve(args),
            NR_EXIT => task::syscall::sys_exit(args),
            NR_WAIT4 => task::syscall::sys_wait4(args),
            NR_KILL => task::syscall::sys_kill(args),
            NR_FCNTL => fs::syscall::sys_fcntl(args),
            NR_GETCWD => fs::syscall::sys_getcwd(args),
            NR_CHDIR => fs::syscall::sys_chdir(args),
            NR_FCHDIR => fs::syscall::sys_fchdir(args),
            NR_STATFS => fs::syscall::sys_statfs(args),
            NR_FSTATFS => fs::syscall::sys_fstatfs(args),
            NR_SETPGID => task::syscall::sys_setpgid(args),
            NR_GETPPID => task::syscall::sys_getppid(args),
            NR_GETPGRP => task::syscall::sys_getpgrp(args),
            NR_SETSID => task::syscall::sys_setsid(args),
            NR_GETPGID => task::syscall::sys_getpgid(args),
            NR_GETTID => task::syscall::sys_gettid(args),
            NR_TKILL => task::syscall::sys_tkill(args),
            NR_GETDENTS64 => fs::syscall::sys_getdents64(args),
            NR_EXIT_GROUP => task::syscall::sys_exit_group(args),
            NR_TGKILL => task::syscall::sys_tgkill(args),
            NR_DUP3 => fs::syscall::sys_dup3(args),
            NR_PIPE2 => fs::syscall::sys_pipe2(args),
            _ => crate::syscall::sys_ni(args),
        }
    }

    fn syscall_table(&self) -> &'static [SyscallDef] {
        SYSCALL_TABLE
    }
}
