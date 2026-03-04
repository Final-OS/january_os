use super::task_step;
use crate::task;
use crate::{error, kprintln, ok};
use core::sync::atomic::{AtomicUsize, Ordering};

static REAPED_CHILD_PID: AtomicUsize = AtomicUsize::new(0);
static REAPED_CHILD_CODE: AtomicUsize = AtomicUsize::new(usize::MAX);

pub(super) fn run() {
    task_step("wait_reap: reset result slots");
    REAPED_CHILD_PID.store(0, Ordering::SeqCst);
    REAPED_CHILD_CODE.store(usize::MAX, Ordering::SeqCst);

    task_step("wait_reap: spawn parent thread");
    task::spawn_kernel_thread("task_wait_parent", wait_parent_thread);

    task_step("wait_reap: polling scheduler for reaped result");
    for _ in 0..48 {
        task::scheduler::schedule();
        if REAPED_CHILD_PID.load(Ordering::SeqCst) != 0 {
            break;
        }
    }

    let pid = REAPED_CHILD_PID.load(Ordering::SeqCst);
    let code = REAPED_CHILD_CODE.load(Ordering::SeqCst);
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/task] wait_reap result: pid={} code={}", pid, code);
    }

    if pid != 0 && code == 0 {
        ok!("task: wait/reap OK (pid={}, code={})", pid, code);
    } else {
        error!("task: wait/reap FAIL (pid={}, code={})", pid, code);
    }
}

extern "C" fn wait_parent_thread() {
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/task] wait_parent: spawn child");
    }
    task::spawn_kernel_thread("task_wait_child", wait_child_thread);

    for _ in 0..24 {
        if let Some((pid, code)) = task::wait_child(None) {
            if crate::config::DEBUG_VERBOSE {
                kprintln!(
                    "[test/task] wait_parent: reaped pid={} code={}",
                    pid.0,
                    code
                );
            }
            REAPED_CHILD_PID.store(pid.0, Ordering::SeqCst);
            REAPED_CHILD_CODE.store(code as usize, Ordering::SeqCst);
            return;
        }
        task::scheduler::schedule();
    }
}

extern "C" fn wait_child_thread() {}
