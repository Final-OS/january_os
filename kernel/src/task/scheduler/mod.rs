//! 调度器 (Scheduler)

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use crate::sync::Mutex;
use super::task::{Task, TaskStatus};
use super::processor::{PROCESSOR, do_switch};

/// 全局调度器
pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

/// 空闲上下文栈指针
///
/// 当非任务上下文（如 shell）首次调用 schedule() 时，
/// 保存调用者的上下文到此处。当所有就绪任务完成后，
/// 切换回此上下文以恢复调用者。
static mut IDLE_CONTEXT_SP: usize = 0;

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

    pub fn add_task(&mut self, task: Arc<Mutex<Task>>) {
        task.lock().status = TaskStatus::Ready;
        self.ready_queue.push_back(task);
    }

    pub fn pick_next(&mut self) -> Option<Arc<Mutex<Task>>> {
        self.ready_queue.pop_front()
    }

    pub fn push_back(&mut self, task: Arc<Mutex<Task>>) {
        self.ready_queue.push_back(task);
    }
}

/// 调度：从就绪队列取下一个任务并切换
///
/// 关键：所有锁必须在 __switch 之前释放。
pub fn schedule() {
    // 1. 从就绪队列取下一个任务
    let next = SCHEDULER.lock().pick_next();
    let next_task = match next {
        Some(t) => t,
        None => {
            // 没有就绪任务
            // 如果当前在任务上下文中且有保存的空闲上下文，切换回去
            unsafe {
                if IDLE_CONTEXT_SP != 0 {
                    let prev_task = PROCESSOR.lock().take_current();
                    if let Some(prev) = prev_task {
                        let prev_ctx_ptr: *mut usize = {
                            let mut t = prev.lock();
                            &mut t.context_sp as *mut usize
                        };
                        let idle_ptr = core::ptr::addr_of_mut!(IDLE_CONTEXT_SP);
                        do_switch(prev_ctx_ptr, idle_ptr as *const usize);
                        return;
                    }
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
        let prev_ctx_ptr: *mut usize = {
            let mut t = prev.lock();
            &mut t.context_sp as *mut usize
        };

        // 放回就绪队列
        {
            let status = prev.lock().status;
            if status != TaskStatus::Exited {
                SCHEDULER.lock().push_back(prev);
            }
        }

        // 5. 所有锁已释放，执行切换
        unsafe { do_switch(prev_ctx_ptr, next_ctx_ptr); }
    } else {
        // 首次调度，没有 prev — 保存调用者上下文到静态变量
        unsafe {
            let idle_ptr = core::ptr::addr_of_mut!(IDLE_CONTEXT_SP);
            do_switch(idle_ptr, next_ctx_ptr);
        }
    }
}
