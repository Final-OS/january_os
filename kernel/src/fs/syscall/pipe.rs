use super::*;

fn write_pipe_fds_user(ptr: usize, read_fd: i32, write_fd: i32) -> Result<(), i32> {
    validate_user_range(ptr, core::mem::size_of::<i32>() * PIPE_FD_COUNT)?;

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
    if let Err(errno) = validate_user_range(pipefd_ptr, core::mem::size_of::<i32>() * PIPE_FD_COUNT)
    {
        return err(errno);
    }

    let pid = match current_pid_raw() {
        Ok(pid) => pid,
        Err(errno) => return err(errno),
    };
    let (read_fd, write_fd) = match fs::runtime::pipe2_for_pid(pid, flags) {
        Ok(fds) => fds,
        Err(errno) => return err(errno),
    };

    if let Err(errno) = write_pipe_fds_user(pipefd_ptr, read_fd, write_fd) {
        let _ = fs::runtime::close_for_pid(pid, read_fd);
        let _ = fs::runtime::close_for_pid(pid, write_fd);
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

    if fd > 2 && fs::runtime::stat_fd(pid, fd).is_err() {
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
