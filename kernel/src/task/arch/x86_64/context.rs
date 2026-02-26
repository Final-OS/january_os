//! Architecture-specific context management for x86_64

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TaskContext {
    // Callee-saved registers pushed by __switch
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rip: u64, // Return address (pushed by call)
}

/// 内核线程退出 trampoline
///
/// 当内核线程的入口函数返回时，ret 会跳到这里。
/// 在栈上布局为: [TaskContext] [kernel_thread_entry] [task_exit_trampoline]
/// __switch 恢复后 ret → kernel_thread_entry，
/// kernel_thread_entry ret → task_exit_trampoline。
extern "C" fn task_exit_trampoline() -> ! {
    // 线程入口函数返回，标记当前任务为 Exited 并让出 CPU
    crate::task::exit_current_task(0);
    crate::task::scheduler::schedule();
    loop {
        core::hint::spin_loop();
    }
}

impl TaskContext {
    /// Create a new context for a kernel thread
    ///
    /// Stack layout (high → low):
    /// ```text
    ///   kstack_top
    ///   [task_exit_trampoline]   ← entry 函数 ret 时的返回地址
    ///   [TaskContext.rip = entry] ← __switch ret 时跳转到 entry
    ///   [TaskContext 其余字段]
    ///   sp ← 返回值
    /// ```
    pub fn init(entry: usize, kstack_top: usize) -> usize {
        let mut sp = kstack_top;

        // Push return address for when entry() returns
        sp -= core::mem::size_of::<u64>();
        unsafe {
            *(sp as *mut u64) = task_exit_trampoline as *const () as u64;
        }

        // Reserve space for TaskContext
        sp -= core::mem::size_of::<TaskContext>();

        let ctx = unsafe { &mut *(sp as *mut TaskContext) };
        *ctx = TaskContext {
            rip: entry as u64,
            ..Default::default()
        };

        sp
    }

    pub fn empty() -> Self {
        Self::default()
    }
}
