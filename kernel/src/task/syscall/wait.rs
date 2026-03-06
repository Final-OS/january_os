use super::*;

pub(crate) fn sys_wait4(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let status_ptr = args.arg1;
    let rusage_ptr = args.arg3;

    let options = match parse_wait_options(args.arg2) {
        Ok(options) => options,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[wait4] invalid options={:#x} raw_pid={}",
                    args.arg2,
                    raw_pid
                );
            }
            return err(errno);
        }
    };

    let target = match parse_wait_pid(raw_pid) {
        WaitPidFilter::Target(target) => target,
        WaitPidFilter::Invalid => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!("\x1b[90m[diag]\x1b[0m[wait4] invalid raw pid {}", raw_pid);
            }
            return err(EINVAL);
        }
    };

    let wait_options = task::WaitChildOptions {
        include_stopped: options.include_stopped,
        include_continued: options.include_continued,
        clone_filter: options.clone_filter,
        current_thread_only: options.current_thread_only,
    };

    match task::wait_event_by_target(target, wait_options, options.nohang) {
        task::WaitEvent::Exited {
            pid,
            exit_code,
            rusage,
        } => {
            let rusage = build_wait_rusage(rusage);
            if let Err(errno) = write_wait_status(status_ptr, encode_exit_status(exit_code)) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[wait4] write status failed ptr={:#x} errno={}",
                        status_ptr,
                        errno
                    );
                }
                return err(errno);
            }
            if let Err(errno) = write_wait_rusage(rusage_ptr, &rusage) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[wait4] write rusage failed ptr={:#x} errno={}",
                        rusage_ptr,
                        errno
                    );
                }
                return err(errno);
            }

            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[wait4] reaped child pid={} target={:?} options={:#x} rusage_ptr={:#x}",
                    pid.0,
                    target,
                    options.raw,
                    rusage_ptr
                );
            }
            ok(pid.0)
        }
        task::WaitEvent::Stopped { pid, signal, rusage } => {
            let rusage = build_wait_rusage(rusage);
            if let Err(errno) = write_wait_status(status_ptr, encode_stopped_status(signal)) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[wait4] write status failed ptr={:#x} errno={} (stopped)",
                        status_ptr,
                        errno
                    );
                }
                return err(errno);
            }
            if let Err(errno) = write_wait_rusage(rusage_ptr, &rusage) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[wait4] write rusage failed ptr={:#x} errno={} (stopped)",
                        rusage_ptr,
                        errno
                    );
                }
                return err(errno);
            }

            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[wait4] observed stopped child pid={} sig={} target={:?} options={:#x}",
                    pid.0,
                    signal,
                    target,
                    options.raw
                );
            }
            ok(pid.0)
        }
        task::WaitEvent::Continued { pid, rusage } => {
            let rusage = build_wait_rusage(rusage);
            if let Err(errno) = write_wait_status(status_ptr, encode_continued_status()) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[wait4] write status failed ptr={:#x} errno={} (continued)",
                        status_ptr,
                        errno
                    );
                }
                return err(errno);
            }
            if let Err(errno) = write_wait_rusage(rusage_ptr, &rusage) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[wait4] write rusage failed ptr={:#x} errno={} (continued)",
                        rusage_ptr,
                        errno
                    );
                }
                return err(errno);
            }

            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[wait4] observed continued child pid={} target={:?} options={:#x}",
                    pid.0,
                    target,
                    options.raw
                );
            }
            ok(pid.0)
        }
        task::WaitEvent::NoMatchedChild => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[wait4] no matched child target={:?}",
                    target
                );
            }
            err(ECHILD)
        }
        task::WaitEvent::StillRunning => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[wait4] nohang: child not exited target={:?}",
                    target
                );
            }
            ok(0)
        }
    }
}
