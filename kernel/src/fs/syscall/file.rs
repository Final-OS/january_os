use super::*;

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

    match fs::runtime::open_for_pid(pid, path.as_str(), flags, mode) {
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
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    let meta = match fs::runtime::stat_path_for_pid(pid, path.as_str()) {
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

    let meta = match fs::runtime::stat_fd(pid, fd) {
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

    if fd == 0 {
        loop {
            if let Some(byte) = read_tty_byte() {
                dequeue_current_stdin_waiter();
                unsafe {
                    core::ptr::write(buf_ptr as *mut u8, byte);
                }
                return ok(1);
            }

            if crate::task::current_task().is_some() {
                if !enqueue_current_stdin_waiter() {
                    return err(EAGAIN);
                }
                crate::task::scheduler::schedule();
                continue;
            }

            return err(EAGAIN);
        }
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let io_len = count.min(READ_IO_MAX);
    let mut tmp = vec![0u8; io_len];
    let nonblocking = match fs::runtime::fd_is_nonblocking_for_pid(pid, fd) {
        Ok(v) => v,
        Err(errno) => return err(errno),
    };

    let read_len = loop {
        match fs::runtime::read_for_pid(pid, fd, &mut tmp) {
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

    match fs::runtime::close_for_pid(pid, fd) {
        Ok(()) => ok(0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_lseek(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let offset = args.arg1 as isize as i64;
    let whence = args.arg2 as u32;
    if fd < 0 {
        return err(EBADF);
    }
    if whence != SEEK_SET && whence != SEEK_CUR && whence != SEEK_END {
        return err(EINVAL);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    match fs::runtime::lseek_for_pid(pid, fd, offset, whence) {
        Ok(pos) => ok(pos),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_dup(args: &SyscallArgs) -> SyscallRet {
    let oldfd = args.arg0 as i32;
    if oldfd < 0 {
        return err(EBADF);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    match fs::runtime::dup_for_pid(pid, oldfd, 0, false) {
        Ok(fd) => ok(fd as usize),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_dup2(args: &SyscallArgs) -> SyscallRet {
    let oldfd = args.arg0 as i32;
    let newfd = args.arg1 as i32;
    if oldfd < 0 || newfd < 0 {
        return err(EBADF);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    match fs::runtime::dup2_for_pid(pid, oldfd, newfd, false) {
        Ok(fd) => ok(fd as usize),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_fcntl(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let cmd = args.arg1;
    let arg = args.arg2;
    if fd < 0 {
        return err(EBADF);
    }
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    match cmd {
        F_DUPFD => {
            let min_fd = arg as i32;
            if min_fd < 0 {
                return err(EINVAL);
            }
            match fs::runtime::dup_for_pid(pid, fd, min_fd, false) {
                Ok(new_fd) => ok(new_fd as usize),
                Err(errno) => err(errno),
            }
        }
        F_DUPFD_CLOEXEC => {
            let min_fd = arg as i32;
            if min_fd < 0 {
                return err(EINVAL);
            }
            match fs::runtime::dup_for_pid(pid, fd, min_fd, true) {
                Ok(new_fd) => ok(new_fd as usize),
                Err(errno) => err(errno),
            }
        }
        F_GETFD => match fs::runtime::fcntl_getfd_for_pid(pid, fd) {
            Ok(v) => ok(v as usize),
            Err(errno) => err(errno),
        },
        F_SETFD => {
            let cloexec = (arg as u32 & FD_CLOEXEC) != 0;
            match fs::runtime::fcntl_setfd_for_pid(pid, fd, cloexec) {
                Ok(()) => ok(0),
                Err(errno) => err(errno),
            }
        }
        F_GETFL => match fs::runtime::fcntl_getfl_for_pid(pid, fd) {
            Ok(v) => ok(v as usize),
            Err(errno) => err(errno),
        },
        F_SETFL => {
            let allowed = O_NONBLOCK;
            let mask = arg as u32;
            if (mask & !allowed) != 0 {
                return err(EINVAL);
            }
            match fs::runtime::fcntl_setfl_for_pid(pid, fd, mask) {
                Ok(()) => ok(0),
                Err(errno) => err(errno),
            }
        }
        _ => err(ENOSYS),
    }
}

pub(crate) fn sys_chdir(args: &SyscallArgs) -> SyscallRet {
    let path_ptr = args.arg0;
    let path = match unsafe { read_user_cstring(path_ptr, PATH_MAX) } {
        Ok(path) => path,
        Err(errno) => return err(errno),
    };
    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    match fs::runtime::chdir_for_pid(pid, path.as_str()) {
        Ok(()) => ok(0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_getcwd(args: &SyscallArgs) -> SyscallRet {
    let buf_ptr = args.arg0;
    let size = args.arg1;
    if size == 0 {
        return err(EINVAL);
    }
    if buf_ptr == 0 {
        return err(EFAULT);
    }
    if let Err(errno) = validate_user_range(buf_ptr, size) {
        return err(errno);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    let cwd = fs::runtime::getcwd_for_pid(pid);
    let need = cwd.len().saturating_add(1);
    if need > size {
        return err(ERANGE);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(cwd.as_ptr(), buf_ptr as *mut u8, cwd.len());
        core::ptr::write((buf_ptr + cwd.len()) as *mut u8, 0);
    }
    ok(need)
}

#[inline]
fn align_up(value: usize, align: usize) -> usize {
    value.saturating_add(align - 1) & !(align - 1)
}

pub(crate) fn sys_getdents64(args: &SyscallArgs) -> SyscallRet {
    let fd = args.arg0 as i32;
    let dirp = args.arg1;
    let count = args.arg2;
    if fd < 0 {
        return err(EBADF);
    }
    if count == 0 {
        return ok(0);
    }
    if dirp == 0 {
        return err(EFAULT);
    }
    if let Err(errno) = validate_user_range(dirp, count) {
        return err(errno);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };

    let mut written = 0usize;
    loop {
        let entry = match fs::runtime::peek_dir_entry_for_pid(pid, fd) {
            Ok(v) => v,
            Err(errno) => return err(errno),
        };
        let Some(entry) = entry else {
            break;
        };
        let typ = match entry.file_type {
            DT_DIR => DT_DIR,
            DT_REG => DT_REG,
            _ => DT_UNKNOWN,
        };
        let name_len = entry.name.as_bytes().len();
        let reclen = align_up(core::mem::size_of::<LinuxDirent64Fixed>() + name_len + 1, 8);
        if reclen > count.saturating_sub(written) {
            if written == 0 {
                return err(EINVAL);
            }
            break;
        }

        let base = dirp + written;
        let fixed = LinuxDirent64Fixed {
            d_ino: entry.ino,
            d_off: 0,
            d_reclen: reclen as u16,
            d_type: typ,
        };
        unsafe {
            core::ptr::write(base as *mut LinuxDirent64Fixed, fixed);
            let name_ptr = (base + core::mem::size_of::<LinuxDirent64Fixed>()) as *mut u8;
            core::ptr::copy_nonoverlapping(entry.name.as_ptr(), name_ptr, name_len);
            core::ptr::write(name_ptr.add(name_len), 0);
            let pad_start = base + core::mem::size_of::<LinuxDirent64Fixed>() + name_len + 1;
            let pad_len =
                reclen.saturating_sub(core::mem::size_of::<LinuxDirent64Fixed>() + name_len + 1);
            if pad_len > 0 {
                core::ptr::write_bytes(pad_start as *mut u8, 0, pad_len);
            }
        }

        written = written.saturating_add(reclen);
        if let Err(errno) = fs::runtime::advance_dir_cursor_for_pid(pid, fd, 1) {
            return err(errno);
        }
    }
    ok(written)
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

    match fs::runtime::write_for_pid(pid, fd, &tmp) {
        Ok(n) => ok(n),
        Err(errno) => err(errno),
    }
}

