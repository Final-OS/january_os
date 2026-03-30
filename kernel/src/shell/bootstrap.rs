use crate::{drivers, fs, interrupt, task, warn};
use alloc::string::String;
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

const DEFAULT_BOOT_INIT_PATH: &str = crate::boot::DEFAULT_INITRD_COMMAND;
const SESSION_STATE_IDLE: u8 = 0;
const SESSION_STATE_STARTING: u8 = 1;
const SESSION_STATE_RUNNING: u8 = 2;
const SESSION_STATE_COMPLETED: u8 = 3;
const SESSION_STATE_FAILED: u8 = 4;

const SESSION_MODE_RETURN_TO_KERNEL_SHELL: u8 = 1;
const SESSION_MODE_FATAL_IF_EXITED: u8 = 2;

static USER_SESSION_STATE: AtomicU8 = AtomicU8::new(SESSION_STATE_IDLE);
static USER_SESSION_MODE: AtomicU8 = AtomicU8::new(SESSION_MODE_RETURN_TO_KERNEL_SHELL);
static USER_SESSION_EXIT_CODE: AtomicI32 = AtomicI32::new(0);
static USER_SESSION_CMD: crate::sync::Mutex<Option<String>> = crate::sync::Mutex::new(None);

fn exec_current_user_program(path: &str) -> Result<(), i32> {
    crate::info!("[initrd] launching {}", path);
    let pid = task::current_pid()
        .map(|pid| pid.0)
        .ok_or(crate::errno::ESRCH)?;
    let image = fs::read_all_for_pid(pid, path)?;
    let load_plan = task::build_elf_load_plan(image.as_slice())?;

    let staged_mappings = task::stage_pt_load_mappings(image.as_slice(), &load_plan)?;
    if task::record_current_exec_request(path, 1, 0).is_none() {
        task::rollback_exec_mappings(&staged_mappings);
        return Err(crate::errno::ESRCH);
    }

    if task::set_current_exec_mappings(staged_mappings).is_none() {
        return Err(crate::errno::ESRCH);
    }

    let auxv = task::proc::exec::minimal_auxv(&load_plan);
    let stack_rsp = task::setup_initial_user_stack(
        load_plan.stack_top,
        load_plan.stack_pages,
        &[path],
        &[],
        auxv.as_slice(),
    )?;
    let frame = task::arch::build_user_enter_frame(load_plan.entry, stack_rsp);
    unsafe {
        task::arch::enter_user_mode_iret(&frame);
    }
}

fn session_path() -> String {
    USER_SESSION_CMD
        .lock()
        .clone()
        .unwrap_or_else(|| String::from("/bin/sh"))
}

fn resolve_boot_user_program(path: &str) -> String {
    let requested = path.trim();
    if requested.is_empty() {
        return String::from(DEFAULT_BOOT_INIT_PATH);
    }
    String::from(requested)
}

fn finish_supervisor(state: u8, exit_code: i32) -> ! {
    USER_SESSION_EXIT_CODE.store(exit_code, Ordering::Release);
    USER_SESSION_STATE.store(state, Ordering::Release);
    task::exit_current_task(0);
    loop {
        task::sched::schedule();
    }
}

fn fatal_boot_init_exit(path: &str, exit_code: i32) -> ! {
    USER_SESSION_EXIT_CODE.store(exit_code, Ordering::Release);
    USER_SESSION_STATE.store(SESSION_STATE_COMPLETED, Ordering::Release);
    panic!(
        "Attempted to kill init! path={} exit_code={}",
        path, exit_code
    );
}

extern "C" fn user_program_entry() {
    let path = session_path();

    if let Err(errno) = exec_current_user_program(path.as_str()) {
        warn!("[initrd] exec {} failed errno={}", path, errno);
        task::exit_current_task(127);
        loop {
            task::sched::schedule();
        }
    }
}

extern "C" fn user_session_supervisor_entry() {
    let path = session_path();
    let mode = USER_SESSION_MODE.load(Ordering::Acquire);

    let user_task = match task::spawn_kernel_thread_with_mm_mode_checked(
        "user-init",
        user_program_entry,
        task::SpawnMmMode::InheritPrivate,
    ) {
        Some(task_ref) => task_ref,
        None => {
            warn!("[initrd] failed to spawn user task for {}", path);
            if mode == SESSION_MODE_FATAL_IF_EXITED {
                USER_SESSION_STATE.store(SESSION_STATE_FAILED, Ordering::Release);
                panic!("failed to launch init {}", path);
            }
            finish_supervisor(SESSION_STATE_FAILED, 127);
        }
    };

    let user_pid = {
        let task = user_task.lock();
        task.pid
    };
    USER_SESSION_STATE.store(SESSION_STATE_RUNNING, Ordering::Release);

    match task::wait_event_by_target(
        task::WaitTarget::Pid(user_pid),
        task::WaitChildOptions::default(),
        false,
    ) {
        task::WaitEvent::Exited { exit_code, .. } => {
            crate::info!(
                "[initrd] user process exited: path={} pid={} code={}",
                path,
                user_pid.0,
                exit_code
            );
            if mode == SESSION_MODE_FATAL_IF_EXITED {
                fatal_boot_init_exit(path.as_str(), exit_code);
            }
            finish_supervisor(SESSION_STATE_COMPLETED, exit_code);
        }
        other => {
            warn!(
                "[initrd] unexpected wait result for {} pid={} => {:?}",
                path, user_pid.0, other
            );
            if mode == SESSION_MODE_FATAL_IF_EXITED {
                USER_SESSION_STATE.store(SESSION_STATE_FAILED, Ordering::Release);
                panic!("init supervisor lost child {} pid={}", path, user_pid.0);
            }
            finish_supervisor(SESSION_STATE_FAILED, 127);
        }
    }
}

fn drive_runtime_once() {
    drivers::input::poll();
    let _ = fs::wake_stdin_waiters_if_ready();
    task::sched::schedule();
    interrupt::halt_with_interrupts();
}

fn launch_user_session(path: &str, mode: u8) -> bool {
    match USER_SESSION_STATE.load(Ordering::Acquire) {
        SESSION_STATE_STARTING | SESSION_STATE_RUNNING => {
            warn!("[initrd] user session already active");
            return false;
        }
        _ => {}
    }

    USER_SESSION_EXIT_CODE.store(0, Ordering::Release);
    USER_SESSION_MODE.store(mode, Ordering::Release);
    USER_SESSION_STATE.store(SESSION_STATE_STARTING, Ordering::Release);
    *USER_SESSION_CMD.lock() = Some(String::from(path));

    if task::spawn_kernel_thread_with_mm_mode_checked(
        "init-supervisor",
        user_session_supervisor_entry,
        task::SpawnMmMode::InheritInitPrivate,
    )
    .is_none()
    {
        USER_SESSION_STATE.store(SESSION_STATE_FAILED, Ordering::Release);
        warn!("[initrd] failed to spawn supervisor for {}", path);
        return false;
    }

    for _ in 0..4096 {
        drive_runtime_once();
        match USER_SESSION_STATE.load(Ordering::Acquire) {
            SESSION_STATE_RUNNING | SESSION_STATE_COMPLETED => return true,
            SESSION_STATE_FAILED => return false,
            _ => {}
        }
    }

    warn!("[initrd] bootstrap timeout for {}", path);
    false
}

pub(super) fn run_boot_user_init(path: &str) -> ! {
    let resolved = resolve_boot_user_program(path);
    if !launch_user_session(resolved.as_str(), SESSION_MODE_FATAL_IF_EXITED) {
        panic!("failed to launch init {}", resolved);
    }
    run_scheduler_loop();
}

pub(super) fn run_user_session_until_exit(path: &str) -> Option<i32> {
    if !launch_user_session(path, SESSION_MODE_RETURN_TO_KERNEL_SHELL) {
        return None;
    }

    loop {
        match USER_SESSION_STATE.load(Ordering::Acquire) {
            SESSION_STATE_COMPLETED => {
                let exit_code = USER_SESSION_EXIT_CODE.load(Ordering::Acquire);
                *USER_SESSION_CMD.lock() = None;
                USER_SESSION_STATE.store(SESSION_STATE_IDLE, Ordering::Release);
                return Some(exit_code);
            }
            SESSION_STATE_FAILED => {
                *USER_SESSION_CMD.lock() = None;
                USER_SESSION_STATE.store(SESSION_STATE_IDLE, Ordering::Release);
                return None;
            }
            SESSION_STATE_IDLE => return None,
            _ => drive_runtime_once(),
        }
    }
}

pub(super) fn run_scheduler_loop() -> ! {
    loop {
        drive_runtime_once();
    }
}
