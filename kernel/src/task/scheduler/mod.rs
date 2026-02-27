//! 调度器 (Scheduler)

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use crate::interrupt;
use crate::sync::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};
use super::id::ProcessId;
use super::task::{Task, TaskStatus};
use super::processor::{PROCESSOR, do_switch};

/// 全局调度器
pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

/// 空闲上下文栈指针
///
/// 当非任务上下文（如 shell）首次调用 schedule() 时，
/// 保存调用者的上下文到此处。当所有就绪任务完成后，
/// 切换回此上下文以恢复调用者。
static IDLE_CONTEXT_SP: AtomicUsize = AtomicUsize::new(0);

struct DeferredRequeue {
    cpu_id: u32,
    task: Arc<Mutex<Task>>,
}

static DEFERRED_REQUEUE: Mutex<VecDeque<DeferredRequeue>> = Mutex::new(VecDeque::new());

fn current_cpu_id() -> u32 {
    if interrupt::apic_initialized() {
        interrupt::local_apic_id()
    } else {
        0
    }
}

fn enqueue_deferred_requeue(task: Arc<Mutex<Task>>) {
    let cpu_id = current_cpu_id();
    DEFERRED_REQUEUE
        .lock()
        .push_back(DeferredRequeue { cpu_id, task });
}

fn flush_deferred_requeue_for_current_cpu() {
    let cpu_id = current_cpu_id();
    let mut deferred = DEFERRED_REQUEUE.lock();
    if deferred.is_empty() {
        return;
    }

    let mut ready_tasks: VecDeque<Arc<Mutex<Task>>> = VecDeque::new();
    let mut retained: VecDeque<DeferredRequeue> = VecDeque::new();

    while let Some(item) = deferred.pop_front() {
        if item.cpu_id == cpu_id {
            ready_tasks.push_back(item.task);
        } else {
            retained.push_back(item);
        }
    }
    *deferred = retained;
    drop(deferred);

    if ready_tasks.is_empty() {
        return;
    }

    let mut scheduler = SCHEDULER.lock();
    while let Some(task) = ready_tasks.pop_front() {
        scheduler.push_back(task);
    }
}

/// Round-Robin 调度器
pub struct Scheduler {
    ready_queue: VecDeque<Arc<Mutex<Task>>>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }

    fn contains_task(&self, target: &Arc<Mutex<Task>>) -> bool {
        self.ready_queue
            .iter()
            .any(|queued| Arc::ptr_eq(queued, target))
    }

    fn enqueue_task(&mut self, task: Arc<Mutex<Task>>) {
        {
            let mut task_guard = task.lock();
            if task_guard.status == TaskStatus::Exited {
                return;
            }
            task_guard.status = TaskStatus::Ready;
        }

        if self.contains_task(&task) {
            return;
        }

        self.ready_queue.push_back(task);
    }

    pub fn add_task(&mut self, task: Arc<Mutex<Task>>) {
        self.enqueue_task(task);
    }

    pub fn pick_next(&mut self) -> Option<Arc<Mutex<Task>>> {
        let now_ticks = interrupt::timer_ticks();
        let len = self.ready_queue.len();

        for _ in 0..len {
            let Some(task) = self.ready_queue.pop_front() else {
                break;
            };
            let mut task_guard = task.lock();
            match task_guard.status {
                TaskStatus::Exited => {}
                TaskStatus::Ready => {
                    task_guard.status = TaskStatus::Running;
                    task_guard.on_switch_in(now_ticks);
                    drop(task_guard);
                    return Some(task);
                }
                _ => {
                    drop(task_guard);
                    self.ready_queue.push_back(task);
                }
            }
        }

        None
    }

    pub fn push_back(&mut self, task: Arc<Mutex<Task>>) {
        self.enqueue_task(task);
    }

    pub fn remove_tasks_by_pid(&mut self, pid: ProcessId) -> usize {
        let before = self.ready_queue.len();
        self.ready_queue.retain(|task| task.lock().pid != pid);
        before.saturating_sub(self.ready_queue.len())
    }
}

/// 调度：从就绪队列取下一个任务并切换
///
/// 关键：所有锁必须在 __switch 之前释放。
pub fn schedule() {
    flush_deferred_requeue_for_current_cpu();

    // 1. 从就绪队列取下一个任务
    let next = SCHEDULER.lock().pick_next();
    let next_task = match next {
        Some(t) => t,
        None => {
            // 没有就绪任务
            // 如果当前在任务上下文中且有保存的空闲上下文，切换回去
            if IDLE_CONTEXT_SP.load(Ordering::Acquire) != 0 {
                let prev_task = PROCESSOR.lock().take_current();
                if let Some(prev) = prev_task {
                    let now_ticks = interrupt::timer_ticks();
                    let mut should_requeue = false;
                    let prev_ctx_ptr: *mut usize = {
                        let mut t = prev.lock();
                        t.on_switch_out(now_ticks, false);
                        should_requeue = t.status != TaskStatus::Exited;
                        if should_requeue {
                            t.status = TaskStatus::Switching;
                        }
                        &mut t.context_sp as *mut usize
                    };
                    if should_requeue {
                        enqueue_deferred_requeue(prev);
                    }
                    unsafe {
                        let idle_ptr = IDLE_CONTEXT_SP.as_ptr();
                        do_switch(prev_ctx_ptr, idle_ptr as *const usize);
                    }
                    return;
                }
            }
            return;
        }
    };

    // 2. 取出当前任务，设置 next 为 current
    let prev_task = {
        let mut proc = PROCESSOR.lock();
        let prev = proc.take_current();
        proc.set_current(next_task.clone());
        prev
        // proc 锁在这里释放
    };

    // 3. 获取 next 的 context_sp 指针
    let next_ctx_ptr: *const usize = {
        let t = next_task.lock();
        &t.context_sp as *const usize
    };

    // 4. 将 prev 放回就绪队列（如果还活着），并获取其 context_sp 指针
    if let Some(prev) = prev_task {
        let now_ticks = interrupt::timer_ticks();
        let mut should_requeue = false;
        let prev_ctx_ptr: *mut usize = {
            let mut t = prev.lock();
            t.on_switch_out(now_ticks, false);
            should_requeue = t.status != TaskStatus::Exited;
            if should_requeue {
                t.status = TaskStatus::Switching;
            }
            &mut t.context_sp as *mut usize
        };

        // 延迟回入队，确保切换已完成后再让其他 CPU 可见。
        if should_requeue {
            enqueue_deferred_requeue(prev);
        }

        // 5. 所有锁已释放，执行切换
        unsafe { do_switch(prev_ctx_ptr, next_ctx_ptr); }
    } else {
        // 首次调度，没有 prev — 保存调用者上下文到静态变量
        unsafe {
            let idle_ptr = IDLE_CONTEXT_SP.as_ptr();
            do_switch(idle_ptr, next_ctx_ptr);
        }
    }
}

/// 调度器空闲循环
///
/// 当前先提供统一的 idle 入口，后续可在这里接入更复杂的调度策略。
pub fn run_idle() -> ! {
    loop {
        interrupt::halt_with_interrupts();
    }
}
