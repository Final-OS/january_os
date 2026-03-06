//! 内核线程 / 上下文切换测试

mod proc;
mod regression;
mod sched;

use crate::kprintln;
use core::sync::atomic::{AtomicUsize, Ordering};

static TASK_STEP_SEQ: AtomicUsize = AtomicUsize::new(0);

pub(super) fn task_step(msg: &str) {
    let seq = TASK_STEP_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/task][step {}] {}", seq, msg);
    }
}

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    TASK_STEP_SEQ.store(0, Ordering::SeqCst);
    kprintln!("=== Task / Context Switch Test ===");
    task_step("start task test suite");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/task] filter={:?}", filter);
    }

    match filter {
        None | Some("all") => {
            task_step("run case=switch");
            sched::run_context_switch();
            task_step("run case=wait");
            proc::run_wait_reap();
            task_step("run case=usermode");
            proc::run_usermode();
            task_step("run case=fork");
            proc::run_fork();
        }
        Some("safe") => {
            task_step("run case=switch");
            sched::run_context_switch();
            task_step("run case=wait");
            proc::run_wait_reap();
        }
        Some("regression") => {
            task_step("run case=regression");
            regression::run_runtime();
        }
        Some("switch") => {
            task_step("run case=switch");
            sched::run_context_switch();
        }
        Some("wait") => {
            task_step("run case=wait");
            proc::run_wait_reap();
        }
        Some("usermode") => {
            task_step("run case=usermode");
            proc::run_usermode();
        }
        Some("fork") => {
            task_step("run case=fork");
            proc::run_fork();
        }
        Some("help") | _ => {
            task_step("show help");
            kprintln!("Usage: test task [name]");
            kprintln!("Available task tests:");
            kprintln!("  switch       - context switch test");
            kprintln!("  wait         - wait/reap test");
            kprintln!("  usermode     - explicit usermode exec test");
            kprintln!("  fork         - usermode fork + COW test");
            kprintln!("  regression   - kernel reserve + usermode regression");
            kprintln!("  safe         - run stable task tests (switch + wait)");
            kprintln!("  all          - run all task tests (includes usermode + fork)");
            kprintln!("Note: `test task` defaults to `all`.");
            kprintln!();
            return;
        }
    }

    task_step("task test suite done");
    kprintln!();
}
