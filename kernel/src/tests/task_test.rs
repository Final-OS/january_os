//! 内核线程 / 上下文切换测试

use crate::task;
use crate::{error, kprintln, ok};
use core::sync::atomic::{AtomicUsize, Ordering};

static COUNTER_A: AtomicUsize = AtomicUsize::new(0);
static COUNTER_B: AtomicUsize = AtomicUsize::new(0);
static REAPED_CHILD_PID: AtomicUsize = AtomicUsize::new(0);
static REAPED_CHILD_CODE: AtomicUsize = AtomicUsize::new(usize::MAX);

pub fn run() {
    kprintln!("=== Task / Context Switch Test ===");

    COUNTER_A.store(0, Ordering::SeqCst);
    COUNTER_B.store(0, Ordering::SeqCst);

    // 创建两个内核线程
    task::spawn_kernel_thread("test_a", thread_a);
    task::spawn_kernel_thread("test_b", thread_b);

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
        error!("task: FAIL (A={}, B={}, expected 5 each)", a, b);
    }

    run_wait_reap_test();

    kprintln!();
}

fn run_wait_reap_test() {
    REAPED_CHILD_PID.store(0, Ordering::SeqCst);
    REAPED_CHILD_CODE.store(usize::MAX, Ordering::SeqCst);

    task::spawn_kernel_thread("wait_parent", wait_parent_thread);

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
    task::spawn_kernel_thread("wait_child", wait_child_thread);

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
