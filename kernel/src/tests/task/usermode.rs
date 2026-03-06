use super::task_step;
use crate::fs;
use crate::task;
use crate::{error, kprintln, ok, warn};
use core::sync::atomic::{AtomicUsize, Ordering};

static USERMODE_REAPED_PID: AtomicUsize = AtomicUsize::new(0);
static USERMODE_REAPED_CODE: AtomicUsize = AtomicUsize::new(usize::MAX);
const USERMODE_WAIT_TIMEOUT: usize = usize::MAX - 1;
const TEST_EXEC_PATH: &str = "/tests/task/test_user.elf";

#[cfg(target_arch = "x86_64")]
pub(super) fn run() {
    let _ = run_with_label("usermode exec/switch");
}

#[cfg(target_arch = "x86_64")]
pub(super) fn run_with_label(result_label: &str) -> bool {
    #[inline(never)]
    fn fail_and_exit_current_task(code: i32) -> ! {
        task::exit_current_task(code);
        // 不返回到线程包装层，避免把失败覆盖为成功退出码。
        loop {
            task::scheduler::schedule();
        }
    }

    extern "C" fn usermode_entry_thread() {
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/task] usermode_entry: begin path={}", TEST_EXEC_PATH);
        }

        let Some(pid) = task::current_pid().map(|pid| pid.0) else {
            error!("task: usermode FAIL (missing current pid)");
            fail_and_exit_current_task(127);
        };

        let image = match fs::read_all_for_pid(pid, TEST_EXEC_PATH) {
            Ok(image) => image,
            Err(errno) => {
                error!("task: usermode FAIL (read_all_for_pid errno={})", errno);
                fail_and_exit_current_task(127);
            }
        };

        let load_plan = match task::build_elf_load_plan(image.as_slice()) {
            Ok(plan) => plan,
            Err(errno) => {
                error!("task: usermode FAIL (build_elf_load_plan errno={})", errno);
                fail_and_exit_current_task(127);
            }
        };
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/task] usermode_entry: load_plan ready entry={:#x} stack_top={:#x} segs={}",
                load_plan.entry,
                load_plan.stack_top,
                load_plan.segments.len()
            );
        }

        let staged_mappings = match task::stage_pt_load_mappings(image.as_slice(), &load_plan) {
            Ok(mappings) => mappings,
            Err(errno) => {
                error!(
                    "task: usermode FAIL (stage_pt_load_mappings errno={})",
                    errno
                );
                fail_and_exit_current_task(127);
            }
        };
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/task] usermode_entry: staged mappings count={}",
                staged_mappings.len()
            );
        }

        if task::record_current_exec_request(TEST_EXEC_PATH, 1, 0).is_none() {
            error!("task: usermode FAIL (record_current_exec_request)");
            task::rollback_exec_mappings(&staged_mappings);
            fail_and_exit_current_task(127);
        }

        if task::set_current_exec_mappings(staged_mappings).is_none() {
            error!("task: usermode FAIL (set_current_exec_mappings)");
            fail_and_exit_current_task(127);
        }
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/task] usermode_entry: mappings installed");
        }

        let frame = task::arch::build_user_enter_frame(load_plan.entry, load_plan.stack_top);
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/task] usermode_entry: enter ring3 rip={:#x} rsp={:#x}",
                frame.rip,
                frame.rsp
            );
        }
        unsafe {
            task::arch::enter_user_mode_iret(&frame);
        }
    }

    extern "C" fn usermode_parent_thread() {
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/task] usermode_parent: spawn usermode child");
        }
        let user_task = task::spawn_kernel_thread("task_usermode_child", usermode_entry_thread);
        let user_pid = user_task.lock().pid;
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/task] usermode_parent: child pid={}", user_pid.0);
        }

        for _ in 0..256 {
            if let Some((reaped_pid, exit_code)) = task::wait_child(Some(user_pid)) {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/task] usermode_parent: reaped pid={} code={}",
                        reaped_pid.0,
                        exit_code
                    );
                }
                USERMODE_REAPED_PID.store(reaped_pid.0, Ordering::SeqCst);
                USERMODE_REAPED_CODE.store(exit_code as usize, Ordering::SeqCst);
                return;
            }
            task::scheduler::schedule();
        }

        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/task] usermode_parent: timeout waiting pid={}",
                user_pid.0
            );
        }
        USERMODE_REAPED_PID.store(user_pid.0, Ordering::SeqCst);
        USERMODE_REAPED_CODE.store(USERMODE_WAIT_TIMEOUT, Ordering::SeqCst);
    }

    task_step("usermode: reset result slots");
    USERMODE_REAPED_PID.store(0, Ordering::SeqCst);
    USERMODE_REAPED_CODE.store(usize::MAX, Ordering::SeqCst);
    task_step("usermode: spawn parent thread");
    task::spawn_kernel_thread("task_usermode_parent", usermode_parent_thread);

    task_step("usermode: polling scheduler for parent result");
    for _ in 0..512 {
        let reaped_code = USERMODE_REAPED_CODE.load(Ordering::SeqCst);
        if reaped_code != usize::MAX {
            let reaped_pid = USERMODE_REAPED_PID.load(Ordering::SeqCst);
            if crate::config::DEBUG_VERBOSE {
                kprintln!(
                    "[test/task] usermode final result: pid={} code={}",
                    reaped_pid,
                    reaped_code
                );
            }
            if reaped_code == USERMODE_WAIT_TIMEOUT {
                error!(
                    "task: {} FAIL (timeout waiting pid={})",
                    result_label, reaped_pid
                );
                return false;
            } else if reaped_code == 0 {
                ok!(
                    "task: {} OK (pid={}, code={})",
                    result_label,
                    reaped_pid,
                    reaped_code
                );
                return true;
            } else {
                error!(
                    "task: {} FAIL (pid={}, code={}, expected code=0)",
                    result_label, reaped_pid, reaped_code
                );
                return false;
            }
        }
        task::scheduler::schedule();
    }

    error!("task: {} FAIL (parent monitor timeout)", result_label);
    false
}

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn run() {
    warn!("task: usermode test not supported on this architecture, skip");
}

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn run_with_label(_result_label: &str) -> bool {
    warn!("task: usermode test not supported on this architecture, skip");
    true
}
