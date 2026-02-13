use crate::syscall::{ECHILD, EFAULT, EINVAL, SyscallArgs, SyscallRet, err, ok};
use crate::task;

const WNOHANG: usize = 1;
const WUNTRACED: usize = 2;
const WCONTINUED: usize = 8;
const __WNOTHREAD: usize = 0x2000_0000;
const __WALL: usize = 0x4000_0000;
const __WCLONE: usize = 0x8000_0000;
const WAIT4_SUPPORTED_OPTS: usize =
    WNOHANG | WUNTRACED | WCONTINUED | __WNOTHREAD | __WALL | __WCLONE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wait4Options {
    raw: usize,
    nohang: bool,
    include_stopped: bool,
    include_continued: bool,
    clone_filter: task::WaitCloneFilter,
    current_thread_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitPidFilter {
    Target(task::WaitTarget),
    Invalid,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct TimeVal {
    tv_sec: i64,
    tv_usec: i64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct Rusage {
    ru_utime: TimeVal,
    ru_stime: TimeVal,
    ru_maxrss: i64,
    ru_ixrss: i64,
    ru_idrss: i64,
    ru_isrss: i64,
    ru_minflt: i64,
    ru_majflt: i64,
    ru_nswap: i64,
    ru_inblock: i64,
    ru_oublock: i64,
    ru_msgsnd: i64,
    ru_msgrcv: i64,
    ru_nsignals: i64,
    ru_nvcsw: i64,
    ru_nivcsw: i64,
}

#[inline]
fn parse_wait_pid(raw_pid: isize) -> WaitPidFilter {
    if raw_pid > 0 {
        WaitPidFilter::Target(task::WaitTarget::Pid(task::ProcessId(raw_pid as usize)))
    } else if raw_pid == -1 {
        WaitPidFilter::Target(task::WaitTarget::Any)
    } else if raw_pid == 0 {
        match task::current_pgid() {
            Some(pgid) => WaitPidFilter::Target(task::WaitTarget::Pgid(pgid)),
            None => WaitPidFilter::Invalid,
        }
    } else {
        match raw_pid.checked_neg() {
            Some(negated) if negated > 1 => {
                WaitPidFilter::Target(task::WaitTarget::Pgid(task::ProcessId(negated as usize)))
            }
            _ => WaitPidFilter::Invalid,
        }
    }
}

#[inline]
fn encode_exit_status(exit_code: i32) -> i32 {
    (((exit_code as u32) & 0xff) as i32) << 8
}

#[inline]
fn encode_stopped_status(signal: i32) -> i32 {
    ((((signal as u32) & 0xff) << 8) | 0x7f) as i32
}

#[inline]
fn encode_continued_status() -> i32 {
    0xffff
}

#[inline]
fn saturating_u64_to_i64(value: u64) -> i64 {
    if value > i64::MAX as u64 {
        i64::MAX
    } else {
        value as i64
    }
}

#[inline]
fn ticks_to_timeval(ticks: u64) -> TimeVal {
    let tick_hz = crate::interrupt::TIMER_TICK_HZ;
    if tick_hz == 0 {
        return TimeVal::default();
    }

    let sec = ticks / tick_hz;
    let rem_ticks = ticks % tick_hz;
    let usec = ((rem_ticks as u128) * 1_000_000u128 / (tick_hz as u128)) as u64;

    TimeVal {
        tv_sec: saturating_u64_to_i64(sec),
        tv_usec: saturating_u64_to_i64(usec),
    }
}

fn build_wait_rusage(snapshot: task::WaitRusageSnapshot) -> Rusage {
    Rusage {
        ru_utime: ticks_to_timeval(snapshot.user_ticks),
        ru_stime: ticks_to_timeval(snapshot.system_ticks),
        ru_maxrss: snapshot.max_rss_kb,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: saturating_u64_to_i64(snapshot.minor_faults),
        ru_majflt: saturating_u64_to_i64(snapshot.major_faults),
        ru_nswap: 0,
        ru_inblock: saturating_u64_to_i64(snapshot.inblock),
        ru_oublock: saturating_u64_to_i64(snapshot.oublock),
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: saturating_u64_to_i64(snapshot.signals_delivered),
        ru_nvcsw: saturating_u64_to_i64(snapshot.voluntary_ctxt_switches),
        ru_nivcsw: saturating_u64_to_i64(snapshot.involuntary_ctxt_switches),
    }
}

fn current_wait_rusage(child_pid: task::ProcessId) -> Rusage {
    let snapshot = task::snapshot_observed_child_rusage(child_pid).unwrap_or_default();
    build_wait_rusage(snapshot)
}

fn parse_wait_options(raw: usize) -> Result<Wait4Options, i32> {
    let normalized = (raw as u32) as usize;
    if normalized & !WAIT4_SUPPORTED_OPTS != 0 {
        return Err(EINVAL);
    }

    let clone_filter = if (normalized & __WALL) != 0 {
        task::WaitCloneFilter::All
    } else if (normalized & __WCLONE) != 0 {
        task::WaitCloneFilter::CloneOnly
    } else {
        task::WaitCloneFilter::NonCloneOnly
    };

    Ok(Wait4Options {
        raw: normalized,
        nohang: (normalized & WNOHANG) != 0,
        include_stopped: (normalized & WUNTRACED) != 0,
        include_continued: (normalized & WCONTINUED) != 0,
        clone_filter,
        current_thread_only: (normalized & __WNOTHREAD) != 0,
    })
}

fn validate_write_ptr(ptr: usize, size: usize, align: usize) -> Result<(), i32> {
    if ptr == 0 {
        return Ok(());
    }

    if align == 0 || ptr % align != 0 {
        return Err(EFAULT);
    }

    let last = ptr
        .checked_add(size.saturating_sub(1))
        .ok_or(EFAULT)?;

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

fn write_wait_status(status_ptr: usize, status: i32) -> Result<(), i32> {
    validate_write_ptr(status_ptr, core::mem::size_of::<i32>(), core::mem::align_of::<i32>())?;

    if status_ptr == 0 {
        return Ok(());
    }

    unsafe {
        core::ptr::write(status_ptr as *mut i32, status);
    }

    Ok(())
}

fn write_wait_rusage(rusage_ptr: usize, usage: &Rusage) -> Result<(), i32> {
    validate_write_ptr(
        rusage_ptr,
        core::mem::size_of::<Rusage>(),
        core::mem::align_of::<Rusage>(),
    )?;

    if rusage_ptr == 0 {
        return Ok(());
    }

    unsafe {
        core::ptr::write(rusage_ptr as *mut Rusage, *usage);
    }

    Ok(())
}

pub(crate) fn sys_getpid(_args: &SyscallArgs) -> SyscallRet {
    if let Some(pid) = task::current_pid() {
        ok(pid.0)
    } else {
        err(EINVAL)
    }
}

pub(crate) fn sys_getppid(_args: &SyscallArgs) -> SyscallRet {
    if let Some(ppid) = task::current_ppid() {
        ok(ppid.0)
    } else {
        ok(0)
    }
}

pub(crate) fn sys_gettid(_args: &SyscallArgs) -> SyscallRet {
    if let Some(tid) = task::current_tid() {
        ok(tid.0)
    } else {
        err(EINVAL)
    }
}

pub(crate) fn sys_wait4(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let filter = parse_wait_pid(raw_pid);
    let status_ptr = args.arg1;
    let rusage_ptr = args.arg3;

    let options = match parse_wait_options(args.arg2) {
        Ok(options) => options,
        Err(errno) => {
            crate::kprintln!(
                "[diag][wait4] invalid options={:#x} raw_pid={}",
                args.arg2,
                raw_pid
            );
            return err(errno);
        }
    };

    let mut logged_waiting = false;

    let target = match filter {
        WaitPidFilter::Target(target) => target,
        WaitPidFilter::Invalid => {
            crate::kprintln!("[diag][wait4] invalid raw pid {}", raw_pid);
            return err(EINVAL);
        }
    };

    let wait_options = task::WaitChildOptions {
        include_stopped: options.include_stopped,
        include_continued: options.include_continued,
        clone_filter: options.clone_filter,
        current_thread_only: options.current_thread_only,
    };

    loop {
        match task::wait_child_observe_by_target_with_options(target, wait_options) {
            task::WaitChildObserveResult::Reapable(child_pid, exit_code) => {
                let rusage = current_wait_rusage(child_pid);

                if let Err(errno) = write_wait_status(status_ptr, encode_exit_status(exit_code)) {
                    crate::kprintln!(
                        "[diag][wait4] write status failed ptr={:#x} errno={}",
                        status_ptr,
                        errno
                    );
                    return err(errno);
                }
                if let Err(errno) = write_wait_rusage(rusage_ptr, &rusage) {
                    crate::kprintln!(
                        "[diag][wait4] write rusage failed ptr={:#x} errno={}",
                        rusage_ptr,
                        errno
                    );
                    return err(errno);
                }

                let Some((reaped_pid, _reaped_exit_code)) = task::reap_observed_child(child_pid) else {
                    continue;
                };

                crate::kprintln!(
                    "[diag][wait4] reaped child pid={} target={:?} options={:#x} rusage_ptr={:#x}",
                    reaped_pid.0,
                    target,
                    options.raw,
                    rusage_ptr
                );
                return ok(reaped_pid.0);
            }
            task::WaitChildObserveResult::Stopped(child_pid, signal) => {
                let rusage = current_wait_rusage(child_pid);

                if let Err(errno) = write_wait_status(status_ptr, encode_stopped_status(signal)) {
                    crate::kprintln!(
                        "[diag][wait4] write status failed ptr={:#x} errno={} (stopped)",
                        status_ptr,
                        errno
                    );
                    return err(errno);
                }

                if let Err(errno) = write_wait_rusage(rusage_ptr, &rusage) {
                    crate::kprintln!(
                        "[diag][wait4] write rusage failed ptr={:#x} errno={} (stopped)",
                        rusage_ptr,
                        errno
                    );
                    return err(errno);
                }

                if !task::consume_observed_wait_event(child_pid, task::WaitChildConsumeEvent::Stopped) {
                    continue;
                }

                crate::kprintln!(
                    "[diag][wait4] observed stopped child pid={} sig={} target={:?} options={:#x}",
                    child_pid.0,
                    signal,
                    target,
                    options.raw
                );
                return ok(child_pid.0);
            }
            task::WaitChildObserveResult::Continued(child_pid) => {
                let rusage = current_wait_rusage(child_pid);

                if let Err(errno) = write_wait_status(status_ptr, encode_continued_status()) {
                    crate::kprintln!(
                        "[diag][wait4] write status failed ptr={:#x} errno={} (continued)",
                        status_ptr,
                        errno
                    );
                    return err(errno);
                }

                if let Err(errno) = write_wait_rusage(rusage_ptr, &rusage) {
                    crate::kprintln!(
                        "[diag][wait4] write rusage failed ptr={:#x} errno={} (continued)",
                        rusage_ptr,
                        errno
                    );
                    return err(errno);
                }

                if !task::consume_observed_wait_event(child_pid, task::WaitChildConsumeEvent::Continued) {
                    continue;
                }

                crate::kprintln!(
                    "[diag][wait4] observed continued child pid={} target={:?} options={:#x}",
                    child_pid.0,
                    target,
                    options.raw
                );
                return ok(child_pid.0);
            }
            task::WaitChildObserveResult::NoMatchedChild => {
                crate::kprintln!(
                    "[diag][wait4] no matched child target={:?}",
                    target
                );
                return err(ECHILD);
            }
            task::WaitChildObserveResult::ChildRunning => {
                if options.nohang {
                    crate::kprintln!(
                        "[diag][wait4] nohang: child not exited target={:?}",
                        target
                    );
                    return ok(0);
                }

                if !logged_waiting {
                    crate::kprintln!(
                        "[diag][wait4] blocking wait target={:?}",
                        target
                    );
                    logged_waiting = true;
                }

                task::scheduler::schedule();
            }
        }
    }
}

pub(crate) fn sys_exit(args: &SyscallArgs) -> SyscallRet {
    let exit_code = args.arg0 as i32;
    task::exit_current_task(exit_code);
    task::scheduler::schedule();
    ok(0)
}

pub(crate) fn sys_exit_group(args: &SyscallArgs) -> SyscallRet {
    let exit_code = args.arg0 as i32;
    task::exit_current_process(exit_code);
    task::scheduler::schedule();
    ok(0)
}
