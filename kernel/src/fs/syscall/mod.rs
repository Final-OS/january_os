use crate::drivers;
use crate::drivers::input::hid::keyboard;
use crate::drivers::tty::serial_read_char;
use crate::errno::{
    EAGAIN, EBADF, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ENOSYS, ENOTDIR, ENOTTY, ERANGE, ESRCH,
};
use crate::fs;
use crate::interrupt;
use crate::libs::wait_queue::{WaitMode, WaitQueue};
use crate::sync::IrqSpinLock;
use crate::syscall::{err, ok, SyscallArgs, SyscallRet};
use crate::task;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

const PATH_MAX: usize = 4096;
const READ_IO_MAX: usize = 64 * 1024;
const WRITE_IO_MAX: usize = 64 * 1024;
const O_ACCMODE: u32 = 0o3;
const O_RDONLY: u32 = 0;
const O_NONBLOCK: u32 = 0o4000;
const FD_CLOEXEC: u32 = 1;

const SEEK_SET: u32 = 0;
const SEEK_CUR: u32 = 1;
const SEEK_END: u32 = 2;

const F_DUPFD: usize = 0;
const F_GETFD: usize = 1;
const F_SETFD: usize = 2;
const F_GETFL: usize = 3;
const F_SETFL: usize = 4;
const F_DUPFD_CLOEXEC: usize = 1030;

const DT_UNKNOWN: u8 = 0;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const PIPE_FD_COUNT: usize = 2;
const POLLIN: i16 = 0x0001;
const POLLOUT: i16 = 0x0004;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

const TCGETS: usize = 0x5401;
const TIOCGWINSZ: usize = 0x5413;
const FIONBIO: usize = 0x5421;

static STDIN_WAITERS: IrqSpinLock<WaitQueue> = IrqSpinLock::new(WaitQueue::new());

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxTimespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxStat {
    st_dev: u64,
    st_ino: u64,
    st_nlink: u64,
    st_mode: u32,
    st_uid: u32,
    st_gid: u32,
    __pad0: i32,
    st_rdev: u64,
    st_size: i64,
    st_blksize: i64,
    st_blocks: i64,
    st_atim: LinuxTimespec,
    st_mtim: LinuxTimespec,
    st_ctim: LinuxTimespec,
    __glibc_reserved: [i64; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxPollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxTimeVal {
    tv_sec: i64,
    tv_usec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxTermios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

impl Default for LinuxTermios {
    fn default() -> Self {
        Self {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_line: 0,
            c_cc: [0; 32],
            c_ispeed: 0,
            c_ospeed: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxWinSize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxDirent64Fixed {
    d_ino: u64,
    d_off: i64,
    d_reclen: u16,
    d_type: u8,
}

mod file;
mod pipe;
mod poll;
mod stdin;
mod uaccess;

pub(crate) use file::{
    sys_chdir, sys_close, sys_dup, sys_dup2, sys_fcntl, sys_fstat, sys_getcwd, sys_getdents64,
    sys_lseek, sys_lstat, sys_open, sys_read, sys_stat, sys_write,
};
pub(crate) use pipe::{sys_ioctl, sys_pipe, sys_pipe2};
pub(crate) use poll::{sys_poll, sys_select};
pub(crate) use stdin::{
    dequeue_current_stdin_waiter, enqueue_current_stdin_waiter, read_tty_byte,
    wake_stdin_waiters_if_ready,
};
pub(crate) use uaccess::{
    current_pid_raw, linux_stat_from_fs, read_user_cstring, read_user_struct,
    validate_user_range, write_user_struct,
};
