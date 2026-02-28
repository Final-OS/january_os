//! 调度器 (Scheduler)

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::interrupt;
use crate::sync::Mutex;

use super::id::ProcessId;
use super::processor::{do_switch, replace_current_task, take_current_task};
use super::task::{Task, TaskStatus};

/// 每 CPU 运行队列槽位数（按逻辑 CPU ID 索引）。
const RUNQUEUE_SLOT_COUNT: usize = if crate::config::MAX_CPUS > 0 {
    crate::config::MAX_CPUS
} else {
    1
};
/// 每 CPU 的空闲上下文栈指针槽位。
const IDLE_CONTEXT_SLOT_COUNT: usize = RUNQUEUE_SLOT_COUNT;

#[derive(Default)]
struct RunQueue {
    ready_queue: VecDeque<Arc<Mutex<Task>>>,
}

impl RunQueue {
    const fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
}

static RUNQUEUES: [Mutex<RunQueue>; RUNQUEUE_SLOT_COUNT] =
    [const { Mutex::new(RunQueue::new()) }; RUNQUEUE_SLOT_COUNT];

static IDLE_CONTEXT_SP_SLOTS: [AtomicUsize; IDLE_CONTEXT_SLOT_COUNT] =
    [const { AtomicUsize::new(0) }; IDLE_CONTEXT_SLOT_COUNT];

/// 序列化跨队列修改（入队去重、按 PID 删除）。
/// 调度 pick/steal 不依赖该锁，避免全局热点。
static ENQUEUE_SERIALIZE: Mutex<()> = Mutex::new(());

struct DeferredRequeue {
    cpu_id: usize,
    task: Arc<Mutex<Task>>,
}

static DEFERRED_REQUEUE: Mutex<VecDeque<DeferredRequeue>> = Mutex::new(VecDeque::new());

static SCHED_LOCAL_PICKS: AtomicU64 = AtomicU64::new(0);
static SCHED_STEAL_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static SCHED_STEAL_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static SCHED_IDLE_FALLBACKS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default)]
pub struct SchedulerStats {
    pub local_picks: u64,
    pub steal_attempts: u64,
    pub steal_successes: u64,
    pub idle_fallbacks: u64,
}

/// 调度器实例（无内部状态，状态都在静态队列里）。
pub struct Scheduler;

/// 全局调度器
pub static SCHEDULER: Scheduler = Scheduler::new();

#[inline]
fn current_cpu_id() -> usize {
    crate::smp::current_cpu_id()
}

#[inline]
fn active_cpu_slots() -> usize {
    crate::smp::cpu_count().clamp(1, RUNQUEUE_SLOT_COUNT)
}

#[inline]
fn runqueue_slot_index(cpu_id: usize) -> usize {
    let slots = active_cpu_slots();
    if cpu_id < slots {
        cpu_id
    } else {
        0
    }
}

#[inline]
fn runqueue_for_cpu(cpu_id: usize) -> &'static Mutex<RunQueue> {
    &RUNQUEUES[runqueue_slot_index(cpu_id)]
}

#[inline]
fn idle_context_slot() -> &'static AtomicUsize {
    let cpu_id = current_cpu_id();
    let idx = if cpu_id < IDLE_CONTEXT_SLOT_COUNT { cpu_id } else { 0 };
    &IDLE_CONTEXT_SP_SLOTS[idx]
}

#[inline]
fn current_cr3_pgd() -> u64 {
    crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK
}

#[inline]
fn init_mm_pgd() -> u64 {
    unsafe { (*crate::mm::init_mm_ptr()).pgd }
}

fn task_mm_pgd(task: &Arc<Mutex<Task>>) -> u64 {
    let pid = {
        let guard = task.lock();
        guard.pid
    };

    let mm_ptr = super::manager::find_process_by_pid(pid)
        .map(|process| process.lock().mm as *mut crate::mm::Mm)
        .unwrap_or(crate::mm::init_mm_ptr());

    if mm_ptr.is_null() {
        init_mm_pgd()
    } else {
        unsafe { (*mm_ptr).pgd }
    }
}

#[inline]
fn switch_to_mm_pgd_if_needed(target_pgd: u64) {
    if target_pgd == 0 {
        return;
    }

    if current_cr3_pgd() != target_pgd {
        unsafe {
            crate::mm::arch::write_cr3(target_pgd);
        }
    }
}

#[inline]
fn mark_task_ready_if_schedulable(task: &Arc<Mutex<Task>>) -> bool {
    let mut task_guard = task.lock();
    if task_guard.status == TaskStatus::Exited || task_guard.status == TaskStatus::Running {
        return false;
    }
    task_guard.status = TaskStatus::Ready;
    true
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

    while let Some(task) = ready_tasks.pop_front() {
        SCHEDULER.push_back(task);
    }
}

impl Scheduler {
    pub const fn new() -> Self {
        Self
    }

    fn contains_task_anywhere(&self, target: &Arc<Mutex<Task>>) -> bool {
        for slot in 0..active_cpu_slots() {
            let rq = RUNQUEUES[slot].lock();
            if rq.ready_queue.iter().any(|queued| Arc::ptr_eq(queued, target)) {
                return true;
            }
        }
        false
    }

    fn enqueue_on_cpu(&self, task: Arc<Mutex<Task>>, cpu_id: usize) {
        if !mark_task_ready_if_schedulable(&task) {
            return;
        }

        let _serialize_guard = ENQUEUE_SERIALIZE.lock();
        if self.contains_task_anywhere(&task) {
            return;
        }

        let mut rq = runqueue_for_cpu(cpu_id).lock();
        if mark_task_ready_if_schedulable(&task) {
            rq.ready_queue.push_back(task);
        }
    }

    pub fn add_task(&self, task: Arc<Mutex<Task>>) {
        self.enqueue_on_cpu(task, current_cpu_id());
    }

    pub fn push_back(&self, task: Arc<Mutex<Task>>) {
        self.enqueue_on_cpu(task, current_cpu_id());
    }

    fn pick_next_from_slot(&self, slot: usize, now_ticks: u64) -> Option<Arc<Mutex<Task>>> {
        let mut rq = RUNQUEUES[slot].lock();
        let len = rq.ready_queue.len();
        for _ in 0..len {
            let Some(task) = rq.ready_queue.pop_front() else {
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
                    rq.ready_queue.push_back(task);
                }
            }
        }
        None
    }

    fn steal_from_slot(&self, slot: usize, now_ticks: u64) -> Option<Arc<Mutex<Task>>> {
        let mut rq = RUNQUEUES[slot].lock();
        let len = rq.ready_queue.len();
        for _ in 0..len {
            let Some(task) = rq.ready_queue.pop_back() else {
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
                    rq.ready_queue.push_front(task);
                }
            }
        }
        None
    }

    pub fn pick_next(&self) -> Option<Arc<Mutex<Task>>> {
        let now_ticks = interrupt::timer_ticks();
        let local_slot = runqueue_slot_index(current_cpu_id());

        if let Some(task) = self.pick_next_from_slot(local_slot, now_ticks) {
            SCHED_LOCAL_PICKS.fetch_add(1, Ordering::Relaxed);
            return Some(task);
        }

        let slots = active_cpu_slots();
        for offset in 1..slots {
            let victim = (local_slot + offset) % slots;
            SCHED_STEAL_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
            if let Some(task) = self.steal_from_slot(victim, now_ticks) {
                SCHED_STEAL_SUCCESSES.fetch_add(1, Ordering::Relaxed);
                return Some(task);
            }
        }

        None
    }

    pub fn remove_tasks_by_pid(&self, pid: ProcessId) -> usize {
        let _serialize_guard = ENQUEUE_SERIALIZE.lock();
        let mut removed = 0usize;

        for slot in 0..active_cpu_slots() {
            let mut rq = RUNQUEUES[slot].lock();
            let before = rq.ready_queue.len();
            rq.ready_queue.retain(|task| task.lock().pid != pid);
            removed = removed.saturating_add(before.saturating_sub(rq.ready_queue.len()));
        }

        removed
    }
}

pub fn snapshot_stats() -> SchedulerStats {
    SchedulerStats {
        local_picks: SCHED_LOCAL_PICKS.load(Ordering::Relaxed),
        steal_attempts: SCHED_STEAL_ATTEMPTS.load(Ordering::Relaxed),
        steal_successes: SCHED_STEAL_SUCCESSES.load(Ordering::Relaxed),
        idle_fallbacks: SCHED_IDLE_FALLBACKS.load(Ordering::Relaxed),
    }
}

/// 调度：从就绪队列取下一个任务并切换
///
/// 关键：所有锁必须在 `__switch` 之前释放。
pub fn schedule() {
    flush_deferred_requeue_for_current_cpu();

    // 1. 从当前 CPU 本地队列取任务；无任务时尝试窃取其他 CPU 队列。
    let next = SCHEDULER.pick_next();
    let next_task = match next {
        Some(t) => t,
        None => {
            SCHED_IDLE_FALLBACKS.fetch_add(1, Ordering::Relaxed);
            // 没有就绪任务
            // 如果当前在任务上下文中且有保存的空闲上下文，切换回去
            let idle_slot = idle_context_slot();
            if idle_slot.load(Ordering::Acquire) != 0 {
                let prev_task = take_current_task();
                if let Some(prev) = prev_task {
                    let now_ticks = interrupt::timer_ticks();
                    let mut should_requeue = false;
                    let prev_ctx_ptr: *mut usize = {
                        let mut t = prev.lock();
                        t.on_switch_out(now_ticks, false);
                        if t.status != TaskStatus::Exited {
                            t.status = TaskStatus::Switching;
                            should_requeue = true;
                        }
                        &mut t.context_sp as *mut usize
                    };

                    if should_requeue {
                        enqueue_deferred_requeue(prev);
                    }

                    switch_to_mm_pgd_if_needed(init_mm_pgd());

                    unsafe {
                        let idle_ptr = idle_slot.as_ptr();
                        do_switch(prev_ctx_ptr, idle_ptr as *const usize);
                    }
                    return;
                }
            }
            return;
        }
    };

    // 2. 取出当前任务，设置 next 为 current
    let prev_task = replace_current_task(next_task.clone());

    // 3. 获取 next 的 context_sp 指针
    let next_ctx_ptr: *const usize = {
        let t = next_task.lock();
        &t.context_sp as *const usize
    };

    // 4. 将 prev 放回当前 CPU 的延迟回队（如果还活着），并获取其 context_sp 指针
    if let Some(prev) = prev_task {
        let now_ticks = interrupt::timer_ticks();
        let mut should_requeue = false;
        let prev_ctx_ptr: *mut usize = {
            let mut t = prev.lock();
            t.on_switch_out(now_ticks, false);
            if t.status != TaskStatus::Exited {
                t.status = TaskStatus::Switching;
                should_requeue = true;
            }
            &mut t.context_sp as *mut usize
        };

        if should_requeue {
            enqueue_deferred_requeue(prev);
        }

        switch_to_mm_pgd_if_needed(task_mm_pgd(&next_task));

        // 5. 所有锁已释放，执行切换
        unsafe { do_switch(prev_ctx_ptr, next_ctx_ptr) };
    } else {
        // 首次调度，没有 prev：保存调用者上下文到当前 CPU 的 idle 槽位
        switch_to_mm_pgd_if_needed(task_mm_pgd(&next_task));
        unsafe {
            let idle_ptr = idle_context_slot().as_ptr();
            do_switch(idle_ptr, next_ctx_ptr);
        }
    }
}

/// 调度器空闲循环
pub fn run_idle() -> ! {
    loop {
        interrupt::halt_with_interrupts();
    }
}
