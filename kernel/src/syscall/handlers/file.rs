use crate::fs;
use crate::syscall::{
    EAGAIN, EBADF, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ENOTTY, ESRCH, SyscallArgs, SyscallRet, err, ok,
};
use crate::task;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use core::mem::size_of;

const PATH_MAX: usize = 4096;
const READ_IO_MAX: usize = 64 * 1024;
const WRITE_IO_MAX: usize = 64 * 1024;
const O_ACCMODE: u32 = 0o3;
const O_RDONLY: u32 = 0;
const PIPE_FD_COUNT: usize = 2;
const POLLIN: i16 = 0x0001;
const POLLOUT: i16 = 0x0004;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

const TCGETS: usize = 0x5401;
const TIOCGWINSZ: usize = 0x5413;
const FIONBIO: usize = 0x5421;

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

#[inline]
fn linux_stat_from_fs(meta: fs::FsStat) -> LinuxStat {
    LinuxStat {
        st_dev: meta.dev,
        st_ino: meta.ino,
        st_nlink: meta.nlink,
        st_mode: meta.mode,
        st_uid: 0,
        st_gid: 0,
        __pad0: 0,
        st_rdev: meta.rdev,
        st_size: meta.size,
        st_blksize: meta.blksize,
        st_blocks: meta.blocks,
        st_atim: LinuxTimespec::default(),
        st_mtim: LinuxTimespec::default(),
        st_ctim: LinuxTimespec::default(),
        __glibc_reserved: [0; 3],
    }
}

#[inline]
fn write_user_struct<T: Copy>(ptr: usize, value: &T) -> Result<(), i32> {
    validate_user_range(ptr, size_of::<T>())?;
    unsafe {
        core::ptr::write(ptr as *mut T, *value);
    }
    Ok(())
}

#[inline]
fn read_user_struct<T: Copy>(ptr: usize) -> Result<T, i32> {
    validate_user_range(ptr, size_of::<T>())?;
    Ok(unsafe { core::ptr::read(ptr as *const T) })
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

pub(crate) fn sys_stat(args: &SyscallArgs) -> SyscallRet {
    let path_ptr = args.arg0;
    let statbuf_ptr = args.arg1;
    if statbuf_ptr == 0 {
        return err(EFAULT);
    }

    let path = match unsafe { read_user_cstring(path_ptr, PATH_MAX) } {
        Ok(path) => path,
        Err(errno) => return err(errno),
    };
    let meta = match fs::stat_path(path.as_str()) {
        Ok(meta) => meta,
        Err(errno) => return err(errno),
    };
    let st = linux_stat_from_fs(meta);
    match write_user_struct(statbuf_ptr, &st) {
        Ok(()) => ok(0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_lstat(args: &SyscallArgs) -> SyscallRet {
    sys_stat(args)
}

pub(crate) fn sys_fstat(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let statbuf_ptr = args.arg1;
    if fd < 0 {
        return err(EBADF);
    }
    if statbuf_ptr == 0 {
        return err(EFAULT);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let meta = match fs::stat_fd(pid, fd) {
        Ok(meta) => meta,
        Err(errno) => return err(errno),
    };
    let st = linux_stat_from_fs(meta);
    match write_user_struct(statbuf_ptr, &st) {
        Ok(()) => ok(0),
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
    let nonblocking = match fs::fd_is_nonblocking_for_pid(pid, fd) {
        Ok(v) => v,
        Err(errno) => return err(errno),
    };

    let read_len = loop {
        match fs::read_for_pid(pid, fd, &mut tmp) {
            Ok(n) => break n,
            Err(EAGAIN) if !nonblocking => {
                if crate::interrupt::interrupts_enabled() && crate::task::current_task().is_some() {
                    crate::task::scheduler::schedule();
                    continue;
                }
                return err(EAGAIN);
            }
            Err(errno) => return err(errno),
        }
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

pub(crate) fn sys_ioctl(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let req = args.arg1;
    let argp = args.arg2;
    if fd < 0 {
        return err(EBADF);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    if fd > 2 && fs::stat_fd(pid, fd).is_err() {
        return err(EBADF);
    }

    match req {
        TIOCGWINSZ => {
            if argp == 0 {
                return err(EFAULT);
            }
            let winsz = LinuxWinSize {
                ws_row: 25,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            match write_user_struct(argp, &winsz) {
                Ok(()) => ok(0),
                Err(errno) => err(errno),
            }
        }
        TCGETS => {
            if argp == 0 {
                return err(EFAULT);
            }
            let termios = LinuxTermios::default();
            match write_user_struct(argp, &termios) {
                Ok(()) => ok(0),
                Err(errno) => err(errno),
            }
        }
        FIONBIO => {
            if argp == 0 {
                return err(EFAULT);
            }
            match read_user_struct::<i32>(argp) {
                Ok(_value) => ok(0),
                Err(errno) => err(errno),
            }
        }
        _ => err(ENOTTY),
    }
}

fn poll_once_for_pid(pid: usize, entries: &mut [LinuxPollFd]) -> Result<usize, i32> {
    let mut ready = 0usize;
    for entry in entries.iter_mut() {
        entry.revents = 0;
        if entry.fd < 0 {
            continue;
        }

        match fs::poll_revents_for_pid(pid, entry.fd, entry.events) {
            Ok(revents) => {
                entry.revents = revents;
                if revents != 0 {
                    ready = ready.saturating_add(1);
                }
            }
            Err(EBADF) => {
                entry.revents = POLLNVAL;
                ready = ready.saturating_add(1);
            }
            Err(errno) => return Err(errno),
        }
    }
    Ok(ready)
}

#[inline]
fn timeout_ms_to_deadline_ticks(timeout_ms: i32) -> Option<u64> {
    if timeout_ms < 0 {
        return None;
    }
    let now = crate::interrupt::timer_ticks();
    if timeout_ms == 0 {
        return Some(now);
    }
    let hz = crate::interrupt::TIMER_TICK_HZ;
    let ms = timeout_ms as u64;
    let ticks = ms
        .saturating_mul(hz)
        .saturating_add(999)
        .saturating_div(1000)
        .max(1);
    Some(now.saturating_add(ticks))
}

pub(crate) fn sys_poll(args: &SyscallArgs) -> SyscallRet {
    let fds_ptr = args.arg0;
    let nfds = args.arg1;
    let timeout_ms_raw = args.arg2;
    if nfds > 16_384 {
        return err(EINVAL);
    }
    if timeout_ms_raw > i32::MAX as usize {
        return err(EINVAL);
    }
    if nfds > 0 && fds_ptr == 0 {
        return err(EFAULT);
    }

    let timeout_ms = timeout_ms_raw as i32;
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let bytes = nfds.saturating_mul(size_of::<LinuxPollFd>());
    if bytes > 0 {
        if let Err(errno) = validate_user_range(fds_ptr, bytes) {
            return err(errno);
        }
    }

    let mut entries = vec![LinuxPollFd::default(); nfds];
    if nfds > 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(
                fds_ptr as *const LinuxPollFd,
                entries.as_mut_ptr(),
                nfds,
            );
        }
    }

    let deadline = timeout_ms_to_deadline_ticks(timeout_ms);
    loop {
        let ready = match poll_once_for_pid(pid, &mut entries) {
            Ok(v) => v,
            Err(errno) => return err(errno),
        };
        if ready > 0 {
            break;
        }
        if let Some(dl) = deadline {
            if crate::interrupt::timer_ticks() >= dl {
                break;
            }
        }
        if timeout_ms == 0 {
            break;
        }
        if crate::interrupt::interrupts_enabled() && crate::task::current_task().is_some() {
            crate::task::scheduler::schedule();
        } else {
            break;
        }
    }

    if nfds > 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(entries.as_ptr(), fds_ptr as *mut LinuxPollFd, nfds);
        }
    }
    let ready = entries.iter().filter(|entry| entry.revents != 0).count();
    ok(ready)
}

#[inline]
fn fdset_words(nfds: usize) -> usize {
    let bits = usize::BITS as usize;
    (nfds.saturating_add(bits - 1)) / bits
}

#[inline]
fn fdset_test(bits: &[usize], fd: usize) -> bool {
    let word_bits = usize::BITS as usize;
    let idx = fd / word_bits;
    let bit = fd % word_bits;
    idx < bits.len() && (bits[idx] & (1usize << bit)) != 0
}

#[inline]
fn fdset_set(bits: &mut [usize], fd: usize) {
    let word_bits = usize::BITS as usize;
    let idx = fd / word_bits;
    let bit = fd % word_bits;
    if idx < bits.len() {
        bits[idx] |= 1usize << bit;
    }
}

fn read_fdset(ptr: usize, words: usize) -> Result<Vec<usize>, i32> {
    if ptr == 0 || words == 0 {
        return Ok(Vec::new());
    }
    let bytes = words.saturating_mul(size_of::<usize>());
    validate_user_range(ptr, bytes)?;
    let mut out = vec![0usize; words];
    unsafe {
        core::ptr::copy_nonoverlapping(ptr as *const usize, out.as_mut_ptr(), words);
    }
    Ok(out)
}

fn write_fdset(ptr: usize, bits: &[usize]) -> Result<(), i32> {
    if ptr == 0 || bits.is_empty() {
        return Ok(());
    }
    let bytes = bits.len().saturating_mul(size_of::<usize>());
    validate_user_range(ptr, bytes)?;
    unsafe {
        core::ptr::copy_nonoverlapping(bits.as_ptr(), ptr as *mut usize, bits.len());
    }
    Ok(())
}

fn select_once_for_pid(
    pid: usize,
    nfds: usize,
    read_in: &[usize],
    write_in: &[usize],
    read_out: &mut [usize],
    write_out: &mut [usize],
) -> Result<usize, i32> {
    for word in read_out.iter_mut() {
        *word = 0;
    }
    for word in write_out.iter_mut() {
        *word = 0;
    }

    let mut ready = 0usize;
    for fd in 0..nfds {
        let mut fd_ready = false;

        if !read_in.is_empty() && fdset_test(read_in, fd) {
            match fs::poll_revents_for_pid(pid, fd as i32, POLLIN) {
                Ok(revents) => {
                    if (revents & (POLLIN | POLLERR | POLLHUP)) != 0 {
                        fdset_set(read_out, fd);
                        fd_ready = true;
                    }
                }
                Err(errno) => return Err(errno),
            }
        }

        if !write_in.is_empty() && fdset_test(write_in, fd) {
            match fs::poll_revents_for_pid(pid, fd as i32, POLLOUT) {
                Ok(revents) => {
                    if (revents & (POLLOUT | POLLERR | POLLHUP)) != 0 {
                        fdset_set(write_out, fd);
                        fd_ready = true;
                    }
                }
                Err(errno) => return Err(errno),
            }
        }

        if fd_ready {
            ready = ready.saturating_add(1);
        }
    }

    Ok(ready)
}

pub(crate) fn sys_select(args: &SyscallArgs) -> SyscallRet {
    let nfds = args.arg0;
    let readfds_ptr = args.arg1;
    let writefds_ptr = args.arg2;
    let exceptfds_ptr = args.arg3;
    let timeout_ptr = args.arg4;

    if nfds > 1024 {
        return err(EINVAL);
    }

    let words = fdset_words(nfds);
    let read_in = match read_fdset(readfds_ptr, words) {
        Ok(v) => v,
        Err(errno) => return err(errno),
    };
    let write_in = match read_fdset(writefds_ptr, words) {
        Ok(v) => v,
        Err(errno) => return err(errno),
    };

    let timeout_ms = if timeout_ptr == 0 {
        -1
    } else {
        let tv = match read_user_struct::<LinuxTimeVal>(timeout_ptr) {
            Ok(tv) => tv,
            Err(errno) => return err(errno),
        };
        if tv.tv_sec < 0 || tv.tv_usec < 0 || tv.tv_usec >= 1_000_000 {
            return err(EINVAL);
        }
        let ms = (tv.tv_sec as i128)
            .saturating_mul(1000)
            .saturating_add((tv.tv_usec as i128) / 1000);
        if ms > i32::MAX as i128 {
            i32::MAX
        } else {
            ms as i32
        }
    };

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let mut read_out = vec![0usize; words];
    let mut write_out = vec![0usize; words];
    let deadline = timeout_ms_to_deadline_ticks(timeout_ms);

    loop {
        let ready = match select_once_for_pid(
            pid,
            nfds,
            &read_in,
            &write_in,
            &mut read_out,
            &mut write_out,
        ) {
            Ok(v) => v,
            Err(errno) => return err(errno),
        };
        if ready > 0 {
            if let Err(errno) = write_fdset(readfds_ptr, &read_out) {
                return err(errno);
            }
            if let Err(errno) = write_fdset(writefds_ptr, &write_out) {
                return err(errno);
            }
            if exceptfds_ptr != 0 {
                let zeros = vec![0usize; words];
                if let Err(errno) = write_fdset(exceptfds_ptr, &zeros) {
                    return err(errno);
                }
            }
            return ok(ready);
        }

        if let Some(dl) = deadline {
            if crate::interrupt::timer_ticks() >= dl {
                break;
            }
        }
        if timeout_ms == 0 {
            break;
        }
        if crate::interrupt::interrupts_enabled() && crate::task::current_task().is_some() {
            crate::task::scheduler::schedule();
        } else {
            break;
        }
    }

    if let Err(errno) = write_fdset(readfds_ptr, &read_out) {
        return err(errno);
    }
    if let Err(errno) = write_fdset(writefds_ptr, &write_out) {
        return err(errno);
    }
    if exceptfds_ptr != 0 {
        let zeros = vec![0usize; words];
        if let Err(errno) = write_fdset(exceptfds_ptr, &zeros) {
            return err(errno);
        }
    }

    ok(0)
}
