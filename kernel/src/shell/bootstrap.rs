use crate::{drivers, fs, interrupt, task, warn};
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};

const INIT_STATE_PENDING: u8 = 0;
const INIT_STATE_RUNNING: u8 = 1;
const INIT_STATE_FAILED: u8 = 2;

static INIT_BOOT_STATE: AtomicU8 = AtomicU8::new(INIT_STATE_PENDING);
static INIT_BOOT_CMD: crate::sync::Mutex<Option<String>> = crate::sync::Mutex::new(None);

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

    let stack_rsp =
        task::setup_initial_user_stack(load_plan.stack_top, load_plan.stack_pages, &[path], &[])?;
    let frame = task::arch::build_user_enter_frame(load_plan.entry, stack_rsp);
    INIT_BOOT_STATE.store(INIT_STATE_RUNNING, Ordering::Release);

    unsafe {
        task::arch::enter_user_mode_iret(&frame);
    }
}

extern "C" fn user_init_entry() {
    let path = INIT_BOOT_CMD
        .lock()
        .clone()
        .unwrap_or_else(|| String::from("/bin/sh"));

    if let Err(errno) = exec_current_user_program(path.as_str()) {
        INIT_BOOT_STATE.store(INIT_STATE_FAILED, Ordering::Release);
        warn!("[initrd] exec {} failed errno={}", path, errno);
        task::exit_current_task(127);
        loop {
            task::scheduler::schedule();
        }
    }
}

pub(super) fn try_run_user_init(path: &str) -> bool {
    INIT_BOOT_STATE.store(INIT_STATE_PENDING, Ordering::Release);
    *INIT_BOOT_CMD.lock() = Some(String::from(path));

    if task::spawn_kernel_thread_with_mm_mode_checked(
        "initrd",
        user_init_entry,
        task::SpawnMmMode::InheritPrivate,
    )
    .is_none()
    {
        warn!("[initrd] spawn failed for {}", path);
        return false;
    }

    for _ in 0..2048 {
        task::scheduler::schedule();
        match INIT_BOOT_STATE.load(Ordering::Acquire) {
            INIT_STATE_RUNNING => return true,
            INIT_STATE_FAILED => return false,
            _ => {}
        }
    }

    warn!("[initrd] bootstrap timeout for {}", path);
    false
}

pub(super) fn run_scheduler_loop() -> ! {
    loop {
        drivers::input::poll();
        let _ = crate::fs::wake_stdin_waiters_if_ready();
        task::scheduler::schedule();
        interrupt::halt_with_interrupts();
    }
}
