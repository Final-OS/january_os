//! 内核线程 / 上下文切换测试

use crate::task;
use crate::{error, kprintln, ok, warn};
use core::sync::atomic::{AtomicUsize, Ordering};

static COUNTER_A: AtomicUsize = AtomicUsize::new(0);
static COUNTER_B: AtomicUsize = AtomicUsize::new(0);
static REAPED_CHILD_PID: AtomicUsize = AtomicUsize::new(0);
static REAPED_CHILD_CODE: AtomicUsize = AtomicUsize::new(usize::MAX);
static USERMODE_REAPED_PID: AtomicUsize = AtomicUsize::new(0);
static USERMODE_REAPED_CODE: AtomicUsize = AtomicUsize::new(usize::MAX);

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    kprintln!("=== Task / Context Switch Test ===");

    match filter {
        None | Some("all") => {
            run_context_switch_test();
            run_wait_reap_test();
            run_usermode_exec_test();
        }
        Some("switch") => run_context_switch_test(),
        Some("wait") => run_wait_reap_test(),
        Some("usermode") => run_usermode_exec_test(),
        Some("help") | _ => {
            kprintln!("Usage: test task [name]");
            kprintln!("Available task tests: switch, wait, usermode, all");
            kprintln!();
            return;
        }
    }

    kprintln!();
}

fn run_context_switch_test() {
    COUNTER_A.store(0, Ordering::SeqCst);
    COUNTER_B.store(0, Ordering::SeqCst);

    // 创建两个内核线程
    task::spawn_kernel_thread("task_switch_a", thread_a);
    task::spawn_kernel_thread("task_switch_b", thread_b);

    // 驱动调度：交替运行两个线程直到它们完成
    // 每次 schedule() 会切换到一个就绪线程，
    // 线程 yield 后回到这里继续调度下一个。
    let iterations = 12; // 足够两个线程各跑 5 轮
    for _ in 0..iterations {
        task::scheduler::schedule();
    }

    let a = COUNTER_A.load(Ordering::SeqCst);
    let b = COUNTER_B.load(Ordering::SeqCst);

    if a == 5 && b == 5 {
        ok!("task: context switch OK (A={}, B={})", a, b);
    } else {
        error!("task: context switch FAIL (A={}, B={}, expected 5 each)", a, b);
    }
}

fn run_wait_reap_test() {
    REAPED_CHILD_PID.store(0, Ordering::SeqCst);
    REAPED_CHILD_CODE.store(usize::MAX, Ordering::SeqCst);

    task::spawn_kernel_thread("task_wait_parent", wait_parent_thread);

    for _ in 0..48 {
        task::scheduler::schedule();
        if REAPED_CHILD_PID.load(Ordering::SeqCst) != 0 {
            break;
        }
    }

    let pid = REAPED_CHILD_PID.load(Ordering::SeqCst);
    let code = REAPED_CHILD_CODE.load(Ordering::SeqCst);

    if pid != 0 && code == 0 {
        ok!("task: wait/reap OK (pid={}, code={})", pid, code);
    } else {
        error!("task: wait/reap FAIL (pid={}, code={})", pid, code);
    }
}

#[cfg(target_arch = "x86_64")]
static USERMODE_TEST_ELF: &[u8] = include_bytes!("assets/test_user.elf");

#[cfg(target_arch = "x86_64")]
fn run_usermode_exec_test() {
    const USERMODE_WAIT_TIMEOUT: usize = usize::MAX - 1;

    extern "C" fn usermode_entry_thread() {
        const TEST_EXEC_PATH: &str = "/tests/task/test_user.elf";

        let load_plan = match task::build_elf_load_plan(USERMODE_TEST_ELF) {
            Ok(plan) => plan,
            Err(errno) => {
                error!("task: usermode FAIL (build_elf_load_plan errno={})", errno);
                task::exit_current_task(127);
                task::scheduler::schedule();
                loop {
                    core::hint::spin_loop();
                }
            }
        };

        let staged_mappings = match task::stage_pt_load_mappings(USERMODE_TEST_ELF, &load_plan) {
            Ok(mappings) => mappings,
            Err(errno) => {
                error!("task: usermode FAIL (stage_pt_load_mappings errno={})", errno);
                task::exit_current_task(127);
                task::scheduler::schedule();
                loop {
                    core::hint::spin_loop();
                }
            }
        };

        if task::record_current_exec_request(TEST_EXEC_PATH, 1, 0).is_none() {
            error!("task: usermode FAIL (record_current_exec_request)");
            task::rollback_exec_mappings(&staged_mappings);
            task::exit_current_task(127);
            task::scheduler::schedule();
            loop {
                core::hint::spin_loop();
            }
        }

        if task::set_current_exec_mappings(staged_mappings).is_none() {
            error!("task: usermode FAIL (set_current_exec_mappings)");
            task::exit_current_task(127);
            task::scheduler::schedule();
            loop {
                core::hint::spin_loop();
            }
        }

        let frame = task::arch::build_user_enter_frame(load_plan.entry, load_plan.stack_top);
        unsafe {
            task::arch::enter_user_mode_iret(&frame);
        }
    }

    extern "C" fn usermode_parent_thread() {
        let user_task = task::spawn_kernel_thread("task_usermode_child", usermode_entry_thread);
        let user_pid = user_task.lock().pid;

        for _ in 0..256 {
            if let Some((reaped_pid, exit_code)) = task::wait_child(Some(user_pid)) {
                USERMODE_REAPED_PID.store(reaped_pid.0, Ordering::SeqCst);
                USERMODE_REAPED_CODE.store(exit_code as usize, Ordering::SeqCst);
                return;
            }
            task::scheduler::schedule();
        }

        USERMODE_REAPED_PID.store(user_pid.0, Ordering::SeqCst);
        USERMODE_REAPED_CODE.store(USERMODE_WAIT_TIMEOUT, Ordering::SeqCst);
    }

    USERMODE_REAPED_PID.store(0, Ordering::SeqCst);
    USERMODE_REAPED_CODE.store(usize::MAX, Ordering::SeqCst);
    task::spawn_kernel_thread("task_usermode_parent", usermode_parent_thread);

    for _ in 0..512 {
        let reaped_code = USERMODE_REAPED_CODE.load(Ordering::SeqCst);
        if reaped_code != usize::MAX {
            let reaped_pid = USERMODE_REAPED_PID.load(Ordering::SeqCst);
            if reaped_code == USERMODE_WAIT_TIMEOUT {
                error!(
                    "task: usermode exec/switch FAIL (timeout waiting pid={})",
                    reaped_pid
                );
            } else if reaped_code == 0 {
                ok!(
                    "task: usermode exec/switch OK (pid={}, code={})",
                    reaped_pid,
                    reaped_code
                );
            } else {
                error!(
                    "task: usermode exec/switch FAIL (pid={}, code={}, expected code=0)",
                    reaped_pid,
                    reaped_code
                );
            }
            return;
        }
        task::scheduler::schedule();
    }

    error!("task: usermode exec/switch FAIL (parent monitor timeout)");
}

#[cfg(not(target_arch = "x86_64"))]
fn run_usermode_exec_test() {
    warn!("task: usermode test not supported on this architecture, skip");
}

extern "C" fn thread_a() {
    for i in 0..5 {
        kprintln!("  [thread_a] iteration {}", i);
        COUNTER_A.fetch_add(1, Ordering::SeqCst);
        task::scheduler::schedule();
    }
}

extern "C" fn thread_b() {
    for i in 0..5 {
        kprintln!("  [thread_b] iteration {}", i);
        COUNTER_B.fetch_add(1, Ordering::SeqCst);
        task::scheduler::schedule();
    }
}

extern "C" fn wait_parent_thread() {
    task::spawn_kernel_thread("task_wait_child", wait_child_thread);

    for _ in 0..24 {
        if let Some((pid, code)) = task::wait_child(None) {
            REAPED_CHILD_PID.store(pid.0, Ordering::SeqCst);
            REAPED_CHILD_CODE.store(code as usize, Ordering::SeqCst);
            return;
        }
        task::scheduler::schedule();
    }
}

extern "C" fn wait_child_thread() {}
