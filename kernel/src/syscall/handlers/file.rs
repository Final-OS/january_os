use crate::fs;
use crate::syscall::{
    EBADF, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ESRCH, SyscallArgs, SyscallRet, err, ok,
};
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
const PIPE_FD_COUNT: usize = 2;

#[inline]
fn validate_user_range(ptr: usize, size: usize) -> Result<(), i32> {
    if size == 0 {
        return Ok(());
    }

    let last = ptr.checked_add(size.saturating_sub(1)).ok_or(EFAULT)?;
    let start = ptr as u64;
    let end = last as u64;

    if start < crate::mm::USER_SPACE_START || end < crate::mm::USER_SPACE_START {
        return Err(EFAULT);
    }
    if !crate::mm::is_user_addr(start) || !crate::mm::is_user_addr(end) {
        return Err(EFAULT);
    }

    Ok(())
}

unsafe fn read_user_cstring(ptr: usize, max_len: usize) -> Result<String, i32> {
    if ptr == 0 {
        return Err(EFAULT);
    }

    let mut bytes: Vec<u8> = Vec::new();

    for index in 0..max_len {
        let cur = ptr.checked_add(index).ok_or(EFAULT)?;
        validate_user_range(cur, 1)?;

        let value = unsafe { core::ptr::read(cur as *const u8) };
        if value == 0 {
            if bytes.is_empty() {
                return Err(ENOENT);
            }
            let text = core::str::from_utf8(&bytes).map_err(|_| EINVAL)?;
            return Ok(String::from(text));
        }

        bytes.push(value);
    }

    Err(ENAMETOOLONG)
}

#[inline]
fn current_pid_raw() -> Result<usize, i32> {
    task::current_pid().map(|pid| pid.0).ok_or(ESRCH)
}

pub(crate) fn sys_open(args: &SyscallArgs) -> SyscallRet {
    let path_ptr = args.arg0;
    let flags = args.arg1 as u32;
    let mode = args.arg2 as u16;

    if (flags & O_ACCMODE) != O_RDONLY {
        return err(EINVAL);
    }

    let path = match unsafe { read_user_cstring(path_ptr, PATH_MAX) } {
        Ok(path) => path,
        Err(errno) => return err(errno),
    };

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    match fs::open_for_pid(pid, path.as_str(), flags, mode) {
        Ok(fd) => ok(fd as usize),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_read(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let buf_ptr = args.arg1;
    let count = args.arg2;

    if fd < 0 {
        return err(EBADF);
    }
    if count == 0 {
        return ok(0);
    }
    if buf_ptr == 0 {
        return err(EFAULT);
    }

    if let Err(errno) = validate_user_range(buf_ptr, count) {
        return err(errno);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let io_len = count.min(READ_IO_MAX);
    let mut tmp = vec![0u8; io_len];
    let read_len = match fs::read_for_pid(pid, fd, &mut tmp) {
        Ok(n) => n,
        Err(errno) => return err(errno),
    };

    unsafe {
        core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_ptr as *mut u8, read_len);
    }

    ok(read_len)
}

pub(crate) fn sys_close(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    if fd < 0 {
        return err(EBADF);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    match fs::close_for_pid(pid, fd) {
        Ok(()) => ok(0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_write(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let buf_ptr = args.arg1;
    let count = args.arg2;

    if fd < 0 {
        return err(EBADF);
    }
    if count == 0 {
        return ok(0);
    }
    if buf_ptr == 0 {
        return err(EFAULT);
    }
    if let Err(errno) = validate_user_range(buf_ptr, count) {
        return err(errno);
    }

    let write_len = count.min(WRITE_IO_MAX);
    let mut tmp = vec![0u8; write_len];
    unsafe {
        core::ptr::copy_nonoverlapping(buf_ptr as *const u8, tmp.as_mut_ptr(), write_len);
    }

    if fd == 1 || fd == 2 {
        let mut console = crate::drivers::tty::console::CONSOLE.lock();
        for byte in tmp {
            let _ = console.write_char(byte as char);
        }
        return ok(write_len);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    match fs::write_for_pid(pid, fd, &tmp) {
        Ok(n) => ok(n),
        Err(errno) => err(errno),
    }
}

#[inline]
fn write_pipe_fds_user(ptr: usize, read_fd: i32, write_fd: i32) -> Result<(), i32> {
    validate_user_range(
        ptr,
        core::mem::size_of::<i32>() * PIPE_FD_COUNT,
    )?;

    unsafe {
        core::ptr::write(ptr as *mut i32, read_fd);
        core::ptr::write(
            ptr.saturating_add(core::mem::size_of::<i32>()) as *mut i32,
            write_fd,
        );
    }

    Ok(())
}

#[inline]
fn sys_pipe_impl(pipefd_ptr: usize, flags: u32) -> SyscallRet {
    if pipefd_ptr == 0 {
        return err(EFAULT);
    }
    if let Err(errno) = validate_user_range(
        pipefd_ptr,
        core::mem::size_of::<i32>() * PIPE_FD_COUNT,
    ) {
        return err(errno);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    let (read_fd, write_fd) = match fs::pipe2_for_pid(pid, flags) {
        Ok(fds) => fds,
        Err(errno) => return err(errno),
    };

    if let Err(errno) = write_pipe_fds_user(pipefd_ptr, read_fd, write_fd) {
        let _ = fs::close_for_pid(pid, read_fd);
        let _ = fs::close_for_pid(pid, write_fd);
        return err(errno);
    }

    ok(0)
}

pub(crate) fn sys_pipe(args: &SyscallArgs) -> SyscallRet {
    sys_pipe_impl(args.arg0, 0)
}

pub(crate) fn sys_pipe2(args: &SyscallArgs) -> SyscallRet {
    sys_pipe_impl(args.arg0, args.arg1 as u32)
}
