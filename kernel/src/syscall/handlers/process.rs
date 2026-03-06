use crate::fs;
use crate::syscall::{
    err, ok, SyscallArgs, SyscallRet, E2BIG, ECHILD, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ENOMEM,
    EPERM, ESRCH,
};
use crate::task;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const WNOHANG: usize = 1;
const WUNTRACED: usize = 2;
const WCONTINUED: usize = 8;
const __WNOTHREAD: usize = 0x2000_0000;
const __WALL: usize = 0x4000_0000;
const __WCLONE: usize = 0x8000_0000;
const WAIT4_SUPPORTED_OPTS: usize =
    WNOHANG | WUNTRACED | WCONTINUED | __WNOTHREAD | __WALL | __WCLONE;

const SIGCHLD: i32 = 17;
const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;
const SIGSTOP: i32 = 19;
const SIGCONT: i32 = 18;
const MAX_SIGNAL: i32 = 64;


const EXEC_PATH_MAX: usize = 4096;
const EXEC_ARG_STR_MAX: usize = 4096;
const EXEC_ARG_LIST_MAX: usize = 256;
const EXEC_ENV_LIST_MAX: usize = 256;
const EXEC_TOTAL_BYTES_MAX: usize = 128 * 1024;

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

fn validate_write_ptr(ptr: usize, size: usize, _align: usize) -> Result<(), i32> {
    if ptr == 0 {
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

fn write_wait_status(status_ptr: usize, status: i32) -> Result<(), i32> {
    validate_write_ptr(
        status_ptr,
        core::mem::size_of::<i32>(),
        core::mem::align_of::<i32>(),
    )?;

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

fn validate_read_ptr(ptr: usize, size: usize, _align: usize) -> Result<(), i32> {
    if ptr == 0 {
        return Err(EFAULT);
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

fn read_user_u8(ptr: usize) -> Result<u8, i32> {
    validate_read_ptr(ptr, core::mem::size_of::<u8>(), core::mem::align_of::<u8>())?;

    let value = unsafe { core::ptr::read(ptr as *const u8) };
    Ok(value)
}

fn read_user_usize(ptr: usize) -> Result<usize, i32> {
    validate_read_ptr(
        ptr,
        core::mem::size_of::<usize>(),
        core::mem::align_of::<usize>(),
    )?;

    let value = unsafe { core::ptr::read(ptr as *const usize) };
    Ok(value)
}

fn read_user_cstring(ptr: usize, max_len: usize) -> Result<String, i32> {
    if ptr == 0 {
        return Err(EFAULT);
    }

    let mut bytes: Vec<u8> = Vec::new();
    for offset in 0..max_len {
        let addr = ptr.checked_add(offset).ok_or(EFAULT)?;
        let value = read_user_u8(addr)?;
        if value == 0 {
            let string = core::str::from_utf8(bytes.as_slice()).map_err(|_| EINVAL)?;
            return Ok(String::from(string));
        }

        bytes.push(value);
    }

    Err(ENAMETOOLONG)
}

fn read_user_string_array(
    list_ptr: usize,
    max_items: usize,
    item_len_limit: usize,
) -> Result<(Vec<String>, usize), i32> {
    if list_ptr == 0 {
        return Ok((Vec::new(), 0));
    }

    let mut result: Vec<String> = Vec::new();
    let mut used_bytes = 0usize;
    let ptr_size = core::mem::size_of::<usize>();

    for index in 0..max_items {
        let index_offset = index.checked_mul(ptr_size).ok_or(E2BIG)?;
        let entry_ptr = list_ptr.checked_add(index_offset).ok_or(EFAULT)?;
        let value_ptr = read_user_usize(entry_ptr)?;

        if value_ptr == 0 {
            return Ok((result, used_bytes));
        }

        let value = read_user_cstring(value_ptr, item_len_limit)?;
        used_bytes = used_bytes
            .checked_add(value.len().saturating_add(1))
            .ok_or(E2BIG)?;

        if used_bytes > EXEC_TOTAL_BYTES_MAX {
            return Err(E2BIG);
        }

        result.push(value);
    }

    Err(E2BIG)
}

fn parse_execve_payload(
    path_ptr: usize,
    argv_ptr: usize,
    envp_ptr: usize,
) -> Result<(String, Vec<String>, Vec<String>), i32> {
    let path = read_user_cstring(path_ptr, EXEC_PATH_MAX)?;
    if path.is_empty() {
        return Err(ENOENT);
    }

    let (argv, argv_bytes) = read_user_string_array(argv_ptr, EXEC_ARG_LIST_MAX, EXEC_ARG_STR_MAX)?;
    let (envp, envp_bytes) = read_user_string_array(envp_ptr, EXEC_ENV_LIST_MAX, EXEC_ARG_STR_MAX)?;

    let total_bytes = argv_bytes.checked_add(envp_bytes).ok_or(E2BIG)?;
    if total_bytes > EXEC_TOTAL_BYTES_MAX {
        return Err(E2BIG);
    }

    Ok((path, argv, envp))
}

pub(crate) fn sys_execve(args: &SyscallArgs) -> SyscallRet {
    let path_ptr = args.arg0;
    let argv_ptr = args.arg1;
    let envp_ptr = args.arg2;

    let (path, argv, envp) = match parse_execve_payload(path_ptr, argv_ptr, envp_ptr) {
        Ok(payload) => payload,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] parse failed errno={} path_ptr={:#x} argv_ptr={:#x} envp_ptr={:#x}",
                    errno,
                    path_ptr,
                    argv_ptr,
                    envp_ptr
                );
            }
            return err(errno);
        }
    };

    let pid = match task::current_pid() {
        Some(pid) => pid.0,
        None => return err(ESRCH),
    };

    let image = match fs::runtime::read_all_for_pid(pid, path.as_str()) {
        Ok(image) => image,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] executable image not found path={}",
                    path
                );
            }
            return err(errno);
        }
    };

    let load_plan = match task::build_elf_load_plan(image.as_slice()) {
        Ok(plan) => plan,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] invalid elf path={} errno={} image_len={}",
                    path,
                    errno,
                    image.len()
                );
            }
            return err(errno);
        }
    };

    let map_preview = task::preview_pt_load_mapping(&load_plan);

    let staged_mappings = match task::stage_pt_load_mappings(image.as_slice(), &load_plan) {
        Ok(mapped) => mapped,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] stage PT_LOAD failed path={} errno={} segs={} pages={}",
                    path,
                    errno,
                    map_preview.segment_count,
                    map_preview.total_pages,
                );
            }
            return err(errno);
        }
    };

    if task::record_current_exec_request(path.as_str(), argv.len(), envp.len()).is_none() {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] current process missing while path={} argc={} envc={}",
                path,
                argv.len(),
                envp.len()
            );
        }
        task::rollback_exec_mappings(&staged_mappings);
        return err(ESRCH);
    }

    let argv0 = argv.first().map(|arg| arg.as_str()).unwrap_or("");
    let mapped_segment_pages = staged_mappings
        .iter()
        .filter(|page| page.kind == task::ExecMappedPageKind::Segment)
        .count();
    let mapped_stack_pages = staged_mappings
        .iter()
        .filter(|page| page.kind == task::ExecMappedPageKind::Stack)
        .count();

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] accepted request path={} argc={} envc={} argv0={}",
            path,
            argv.len(),
            envp.len(),
            argv0
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] elf plan path={} image_len={} entry={:#x} segs={} seg_pages={} stack_pages={} total_pages={}",
            path,
            load_plan.image_len,
            load_plan.entry,
            map_preview.segment_count,
            map_preview.segment_pages,
            map_preview.stack_pages,
            map_preview.total_pages,
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] stage mapping done path={} mapped_segment_pages={} mapped_stack_pages={} first_virt={:#x}",
            path,
            mapped_segment_pages,
            mapped_stack_pages,
            staged_mappings.first().map(|page| page.virt).unwrap_or(0),
        );
    }

    let staged_count = staged_mappings.len();
    if let Err(errno) = task::install_current_exec_vmas(&load_plan) {
        task::rollback_exec_mappings(&staged_mappings);
        return err(errno);
    }
    let replaced_pages = match task::set_current_exec_mappings(staged_mappings) {
        Some(replaced) => replaced,
        None => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] install mappings failed path={} staged_pages={}",
                    path,
                    staged_count,
                );
            }
            return err(ESRCH);
        }
    };

    let mut argv_refs: Vec<&str> = if argv.is_empty() {
        vec![path.as_str()]
    } else {
        argv.iter().map(|arg| arg.as_str()).collect()
    };
    let envp_refs: Vec<&str> = envp.iter().map(|arg| arg.as_str()).collect();

    let user_rsp = match task::setup_initial_user_stack(
        load_plan.stack_top,
        load_plan.stack_pages,
        argv_refs.as_slice(),
        envp_refs.as_slice(),
    ) {
        Ok(rsp) => rsp,
        Err(errno) => return err(errno),
    };
    let user_frame = task::arch::build_user_enter_frame(load_plan.entry, user_rsp);

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] user frame rip={:#x} rsp={:#x} cs={:#x} ss={:#x} rflags={:#x}",
            user_frame.rip,
            user_frame.rsp,
            user_frame.cs,
            user_frame.ss,
            user_frame.rflags
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] mappings installed path={} staged_pages={} replaced_pages={}",
            path,
            staged_count,
            replaced_pages,
        );
    }
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] enter ring3 path={} rip={:#x} rsp={:#x}",
            path,
            user_frame.rip,
            user_frame.rsp,
        );
    }

    unsafe {
        task::arch::enter_user_mode_iret(&user_frame);
    }
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

fn parse_signal(raw_sig: usize) -> Result<i32, i32> {
    let sig = raw_sig as i32;
    if sig < 0 || sig > MAX_SIGNAL {
        return Err(EINVAL);
    }
    Ok(sig)
}

fn collect_kill_targets(raw_pid: isize) -> Result<Vec<task::ProcessId>, i32> {
    match raw_pid {
        pid if pid > 0 => {
            let target = task::ProcessId(pid as usize);
            if task::find_process_by_pid(target).is_some() {
                Ok(vec![target])
            } else {
                Err(ESRCH)
            }
        }
        0 => {
            let Some(current_pgid) = task::current_pgid() else {
                return Err(ESRCH);
            };
            let targets = task::manager::process_ids_by_pgid(current_pgid);
            if targets.is_empty() {
                Err(ESRCH)
            } else {
                Ok(targets)
            }
        }
        -1 => {
            let mut targets: Vec<task::ProcessId> = task::manager::all_process_ids()
                .into_iter()
                .filter(|pid| pid.0 != 0)
                .collect();
            targets.sort_by_key(|pid| pid.0);
            if targets.is_empty() {
                Err(ESRCH)
            } else {
                Ok(targets)
            }
        }
        pid => {
            let group_raw = pid.checked_neg().ok_or(EINVAL)?;
            if group_raw <= 1 {
                return Err(EINVAL);
            }
            let group = task::ProcessId(group_raw as usize);
            let targets = task::manager::process_ids_by_pgid(group);
            if targets.is_empty() {
                Err(ESRCH)
            } else {
                Ok(targets)
            }
        }
    }
}

fn signal_process(pid: task::ProcessId, sig: i32) -> Result<bool, i32> {
    let Some(process_ref) = task::find_process_by_pid(pid) else {
        return Err(ESRCH);
    };

    if sig == 0 || sig == SIGCHLD {
        return Ok(false);
    }

    let current_pid = task::current_pid();
    let target_is_current = current_pid == Some(pid);

    match sig {
        SIGTERM | SIGKILL => {
            let exit_code = 128 + sig;
            let tasks = {
                let process = process_ref.lock();
                process.tasks.clone()
            };

            for task_ref in tasks.iter() {
                let mut task = task_ref.lock();
                task.status = task::TaskStatus::Exited;
                task.exit_code = Some(exit_code);
            }

            {
                let mut process = process_ref.lock();
                process.mark_exiting(exit_code);
                process.mark_zombie();
            }

            let removed_ready = task::scheduler::SCHEDULER.remove_tasks_by_pid(pid);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "[90m[diag][0m[signal] terminate pid={} sig={} removed_ready={}",
                    pid.0,
                    sig,
                    removed_ready
                );
            }

            Ok(target_is_current)
        }
        SIGSTOP => {
            let tasks = {
                let mut process = process_ref.lock();
                process.mark_stopped(SIGSTOP);
                process.tasks.clone()
            };

            let mut blocked_tasks = 0usize;
            for task_ref in tasks {
                let mut task = task_ref.lock();
                if task.status != task::TaskStatus::Exited {
                    task.status = task::TaskStatus::Blocked;
                    blocked_tasks = blocked_tasks.saturating_add(1);
                }
            }

            let removed_ready = task::scheduler::SCHEDULER.remove_tasks_by_pid(pid);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "[90m[diag][0m[signal] stop pid={} blocked_tasks={} removed_ready={}",
                    pid.0,
                    blocked_tasks,
                    removed_ready
                );
            }

            Ok(target_is_current)
        }
        SIGCONT => {
            let tasks = {
                let mut process = process_ref.lock();
                process.mark_continued();
                process.tasks.clone()
            };

            let mut resumed_tasks = 0usize;
            for task_ref in tasks {
                let mut should_queue = false;
                {
                    let mut task = task_ref.lock();
                    if task.status == task::TaskStatus::Blocked {
                        task.status = task::TaskStatus::Ready;
                        should_queue = true;
                    }
                }

                if should_queue {
                    task::scheduler::SCHEDULER.add_task(task_ref);
                    resumed_tasks = resumed_tasks.saturating_add(1);
                }
            }

            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "[90m[diag][0m[signal] continue pid={} resumed_tasks={}",
                    pid.0,
                    resumed_tasks
                );
            }
            Ok(false)
        }
        _ => Err(EINVAL),
    }
}

pub(crate) fn sys_clone(args: &SyscallArgs) -> SyscallRet {
    match task::clone_current(args.arg0, args.arg1, args.arg2, args.arg3, args.arg4) {
        Ok(child_pid) => ok(child_pid.0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_fork(_args: &SyscallArgs) -> SyscallRet {
    match task::fork_current() {
        Ok(child_pid) => ok(child_pid.0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_vfork(_args: &SyscallArgs) -> SyscallRet {
    match task::vfork_current() {
        Ok(child_pid) => ok(child_pid.0),
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_getpgid(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    if raw_pid < 0 {
        return err(EINVAL);
    }

    let target_pid = if raw_pid == 0 {
        let Some(current_pid) = task::current_pid() else {
            return err(ESRCH);
        };
        current_pid
    } else {
        task::ProcessId(raw_pid as usize)
    };

    let Some(process) = task::find_process_by_pid(target_pid) else {
        return err(ESRCH);
    };

    ok(process.lock().pgid.0)
}

pub(crate) fn sys_getpgrp(_args: &SyscallArgs) -> SyscallRet {
    if let Some(pgid) = task::current_pgid() {
        ok(pgid.0)
    } else {
        err(ESRCH)
    }
}

pub(crate) fn sys_setpgid(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let raw_pgid = args.arg1 as isize;

    if raw_pid < 0 || raw_pgid < 0 {
        return err(EINVAL);
    }

    let Some(caller_pid) = task::current_pid() else {
        return err(ESRCH);
    };

    let target_pid = if raw_pid == 0 {
        caller_pid
    } else {
        task::ProcessId(raw_pid as usize)
    };

    let Some(target_process_ref) = task::find_process_by_pid(target_pid) else {
        return err(ESRCH);
    };

    if target_pid != caller_pid {
        let target_parent = target_process_ref.lock().parent;
        if target_parent != Some(caller_pid) {
            return err(EPERM);
        }
    }

    let new_pgid = if raw_pgid == 0 {
        target_pid
    } else {
        task::ProcessId(raw_pgid as usize)
    };

    if new_pgid != target_pid {
        let group_exists = task::find_process_by_pid(new_pgid).is_some()
            || !task::manager::process_ids_by_pgid(new_pgid).is_empty();
        if !group_exists {
            return err(EPERM);
        }
    }

    target_process_ref.lock().pgid = new_pgid;

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] setpgid target_pid={} new_pgid={} caller_pid={}",
            target_pid.0,
            new_pgid.0,
            caller_pid.0
        );
    }

    ok(0)
}

pub(crate) fn sys_setsid(_args: &SyscallArgs) -> SyscallRet {
    let Some(caller_pid) = task::current_pid() else {
        return err(ESRCH);
    };

    let Some(process_ref) = task::find_process_by_pid(caller_pid) else {
        return err(ESRCH);
    };

    let current_pgid = process_ref.lock().pgid;
    if current_pgid == caller_pid {
        return err(EPERM);
    }

    let conflicting_group = task::manager::process_ids_by_pgid(caller_pid)
        .into_iter()
        .any(|pid| pid != caller_pid);
    if conflicting_group {
        return err(EPERM);
    }

    process_ref.lock().pgid = caller_pid;

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] setsid pid={} pgid={} done",
            caller_pid.0,
            caller_pid.0
        );
    }

    ok(caller_pid.0)
}

pub(crate) fn sys_kill(args: &SyscallArgs) -> SyscallRet {
    let raw_pid = args.arg0 as isize;
    let sig = match parse_signal(args.arg1) {
        Ok(sig) => sig,
        Err(errno) => return err(errno),
    };

    let targets = match collect_kill_targets(raw_pid) {
        Ok(targets) => targets,
        Err(errno) => return err(errno),
    };

    let mut delivered = 0usize;
    let mut need_schedule = false;

    for pid in targets {
        match signal_process(pid, sig) {
            Ok(resched) => {
                delivered = delivered.saturating_add(1);
                need_schedule |= resched;
            }
            Err(ESRCH) => {}
            Err(errno) => return err(errno),
        }
    }

    if delivered == 0 {
        return err(ESRCH);
    }

    if need_schedule {
        task::scheduler::schedule();
    }

    ok(0)
}

pub(crate) fn sys_tkill(args: &SyscallArgs) -> SyscallRet {
    let raw_tid = args.arg0 as isize;
    if raw_tid <= 0 {
        return err(EINVAL);
    }

    let sig = match parse_signal(args.arg1) {
        Ok(sig) => sig,
        Err(errno) => return err(errno),
    };

    let Some(target_task) = task::manager::find_task_by_tid(task::TaskId(raw_tid as usize)) else {
        return err(ESRCH);
    };
    let target_pid = target_task.lock().pid;

    match signal_process(target_pid, sig) {
        Ok(need_schedule) => {
            if need_schedule {
                task::scheduler::schedule();
            }
            ok(0)
        }
        Err(errno) => err(errno),
    }
}

pub(crate) fn sys_tgkill(args: &SyscallArgs) -> SyscallRet {
    let raw_tgid = args.arg0 as isize;
    let raw_tid = args.arg1 as isize;

    if raw_tgid <= 0 || raw_tid <= 0 {
        return err(EINVAL);
    }

    let sig = match parse_signal(args.arg2) {
        Ok(sig) => sig,
        Err(errno) => return err(errno),
    };

    let Some(target_task) = task::manager::find_task_by_tid(task::TaskId(raw_tid as usize)) else {
        return err(ESRCH);
    };

    let target_pid = target_task.lock().pid;
    if target_pid.0 != raw_tgid as usize {
        return err(ESRCH);
    }

    match signal_process(target_pid, sig) {
        Ok(need_schedule) => {
            if need_schedule {
                task::scheduler::schedule();
            }
            ok(0)
        }
        Err(errno) => err(errno),
    }
}

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
