use super::*;
use crate::common::uaccess::{validate_user_range, write_user_struct};

pub(crate) struct Wait4Options {
    pub(crate) raw: usize,
    pub(crate) nohang: bool,
    pub(crate) include_stopped: bool,
    pub(crate) include_continued: bool,
    pub(crate) clone_filter: task::WaitCloneFilter,
    pub(crate) current_thread_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitPidFilter {
    Target(task::WaitTarget),
    Invalid,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TimeVal {
    tv_sec: i64,
    tv_usec: i64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Rusage {
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
pub(crate) fn parse_wait_pid(raw_pid: isize) -> WaitPidFilter {
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
pub(crate) fn encode_exit_status(exit_code: i32) -> i32 {
    (((exit_code as u32) & 0xff) as i32) << 8
}

#[inline]
pub(crate) fn encode_stopped_status(signal: i32) -> i32 {
    ((((signal as u32) & 0xff) << 8) | 0x7f) as i32
}

#[inline]
pub(crate) fn encode_continued_status() -> i32 {
    0xffff
}

#[inline]
pub(crate) fn saturating_u64_to_i64(value: u64) -> i64 {
    if value > i64::MAX as u64 {
        i64::MAX
    } else {
        value as i64
    }
}

#[inline]
pub(crate) fn ticks_to_timeval(ticks: u64) -> TimeVal {
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

pub(crate) fn build_wait_rusage(snapshot: task::WaitRusageSnapshot) -> Rusage {
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

pub(crate) fn parse_wait_options(raw: usize) -> Result<Wait4Options, i32> {
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

pub(crate) fn validate_write_ptr(ptr: usize, size: usize, _align: usize) -> Result<(), i32> {
    if ptr == 0 {
        return Ok(());
    }
    validate_user_range(ptr, size)
}

pub(crate) fn write_wait_status(status_ptr: usize, status: i32) -> Result<(), i32> {
    validate_write_ptr(
        status_ptr,
        core::mem::size_of::<i32>(),
        core::mem::align_of::<i32>(),
    )?;

    if status_ptr == 0 {
        return Ok(());
    }

    write_user_struct(status_ptr, &status)
}

pub(crate) fn write_wait_rusage(rusage_ptr: usize, usage: &Rusage) -> Result<(), i32> {
    validate_write_ptr(
        rusage_ptr,
        core::mem::size_of::<Rusage>(),
        core::mem::align_of::<Rusage>(),
    )?;

    if rusage_ptr == 0 {
        return Ok(());
    }

    write_user_struct(rusage_ptr, usage)
}
