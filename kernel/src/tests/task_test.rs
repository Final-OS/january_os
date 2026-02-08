//! 内核线程 / 上下文切换测试

use crate::{kprintln, ok, error};
use crate::task;
use core::sync::atomic::{AtomicUsize, Ordering};

static COUNTER_A: AtomicUsize = AtomicUsize::new(0);
static COUNTER_B: AtomicUsize = AtomicUsize::new(0);

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

    kprintln!();
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
