use super::task_step;
use crate::fs;
use crate::task;
use crate::{error, kprintln, ok, warn};
use core::sync::atomic::{AtomicUsize, Ordering};

static FORK_REAPED_PID: AtomicUsize = AtomicUsize::new(0);
static FORK_REAPED_CODE: AtomicUsize = AtomicUsize::new(usize::MAX);
const FORK_WAIT_TIMEOUT: usize = usize::MAX - 2;
const TEST_EXEC_PATH: &str = "/bin/forktest";

#[cfg(target_arch = "x86_64")]
pub(super) fn run() {
    #[inline(never)]
    fn fail_and_exit_current_task(code: i32) -> ! {
        task::exit_current_task(code);
        loop {
            task::sched::schedule();
        }
    }

    extern "C" fn fork_entry_thread() {
        let Some(pid) = task::current_pid().map(|pid| pid.0) else {
            error!("task: fork FAIL (missing current pid)");
            fail_and_exit_current_task(127);
        };

        let image = match fs::runtime::read_all_for_pid(pid, TEST_EXEC_PATH) {
            Ok(image) => image,
            Err(errno) => {
                error!("task: fork FAIL (read_all_for_pid errno={})", errno);
                fail_and_exit_current_task(127);
            }
        };

        let load_plan = match task::build_elf_load_plan(image.as_slice()) {
            Ok(plan) => plan,
            Err(errno) => {
                error!("task: fork FAIL (build_elf_load_plan errno={})", errno);
                fail_and_exit_current_task(127);
            }
        };

        let staged_mappings = match task::stage_pt_load_mappings(image.as_slice(), &load_plan) {
            Ok(mappings) => mappings,
            Err(errno) => {
                error!("task: fork FAIL (stage_pt_load_mappings errno={})", errno);
                fail_and_exit_current_task(127);
            }
        };

        if task::install_current_exec_vmas(&load_plan).is_err() {
            task::rollback_exec_mappings(&staged_mappings);
            fail_and_exit_current_task(127);
        }

        if task::record_current_exec_request(TEST_EXEC_PATH, 1, 0).is_none() {
            task::rollback_exec_mappings(&staged_mappings);
            fail_and_exit_current_task(127);
        }

        if task::set_current_exec_mappings(staged_mappings).is_none() {
            fail_and_exit_current_task(127);
        }

        let frame = task::arch::build_user_enter_frame(load_plan.entry, load_plan.stack_top);
        unsafe {
            task::arch::enter_user_mode_iret(&frame);
        }
    }

    extern "C" fn fork_parent_thread() {
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/task] fork_parent: spawn fork child test");
        }
        let user_task = task::spawn_kernel_thread("task_fork_child", fork_entry_thread);
        let user_pid = user_task.lock().pid;

        for _ in 0..256 {
            if let Some((reaped_pid, exit_code)) = task::wait_child(Some(user_pid)) {
                FORK_REAPED_PID.store(reaped_pid.0, Ordering::SeqCst);
                FORK_REAPED_CODE.store(exit_code as usize, Ordering::SeqCst);
                return;
            }
            task::sched::schedule();
        }

        FORK_REAPED_PID.store(user_pid.0, Ordering::SeqCst);
        FORK_REAPED_CODE.store(FORK_WAIT_TIMEOUT, Ordering::SeqCst);
    }

    task_step("fork: reset result slots");
    FORK_REAPED_PID.store(0, Ordering::SeqCst);
    FORK_REAPED_CODE.store(usize::MAX, Ordering::SeqCst);
    task_step("fork: spawn parent thread");
    task::spawn_kernel_thread("task_fork_parent", fork_parent_thread);

    task_step("fork: polling scheduler for parent result");
    for _ in 0..512 {
        let reaped_code = FORK_REAPED_CODE.load(Ordering::SeqCst);
        if reaped_code != usize::MAX {
            let reaped_pid = FORK_REAPED_PID.load(Ordering::SeqCst);
            if reaped_code == FORK_WAIT_TIMEOUT {
                error!(
                    "task: usermode fork/cow FAIL (timeout waiting pid={})",
                    reaped_pid
                );
                return;
            } else if reaped_code == 0 {
                ok!(
                    "task: usermode fork/cow OK (pid={}, code={})",
                    reaped_pid,
                    reaped_code
                );
                return;
            } else {
                error!(
                    "task: usermode fork/cow FAIL (pid={}, code={}, expected code=0)",
                    reaped_pid, reaped_code
                );
                return;
            }
        }
        task::sched::schedule();
    }

    error!("task: usermode fork/cow FAIL (parent monitor timeout)");
}

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn run() {
    warn!("task: fork test not supported on this architecture, skip");
}
