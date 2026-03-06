use super::*;

fn poll_once_for_pid(pid: usize, entries: &mut [LinuxPollFd]) -> Result<usize, i32> {
    let mut ready = 0usize;
    for entry in entries.iter_mut() {
        entry.revents = 0;
        if entry.fd < 0 {
            continue;
        }

        match fs::runtime::poll_revents_for_pid(pid, entry.fd, entry.events) {
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
            crate::task::sched::schedule();
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
            match fs::runtime::poll_revents_for_pid(pid, fd as i32, POLLIN) {
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
            match fs::runtime::poll_revents_for_pid(pid, fd as i32, POLLOUT) {
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
            crate::task::sched::schedule();
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
