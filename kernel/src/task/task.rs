use super::arch::TaskContext;
use super::id::{ProcessId, TaskId};
use alloc::alloc::{alloc, dealloc, Layout};
use alloc::string::String;
use core::ptr::NonNull;

const KERNEL_STACK_SIZE: usize = 32 * 1024; // 32KB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
    Switching,
    Blocked,
    Exited,
}

pub struct KernelStack {
    ptr: NonNull<u8>,
    layout: Layout,
}

// KernelStack owns the memory and is safe to send/share between threads
unsafe impl Send for KernelStack {}
unsafe impl Sync for KernelStack {}

impl KernelStack {
    pub fn new() -> Option<Self> {
        let layout = Layout::from_size_align(KERNEL_STACK_SIZE, 4096).ok()?;
        // SAFETY: alloc is unsafe, we trust Layout is correct.
        let ptr = unsafe { alloc(layout) };
        NonNull::new(ptr).map(|ptr| Self { ptr, layout })
    }

    pub fn top(&self) -> usize {
        self.ptr.as_ptr() as usize + KERNEL_STACK_SIZE
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

pub struct Task {
    pub id: TaskId,
    pub pid: ProcessId,
    pub ppid: ProcessId,
    pub name: String,
    pub context_sp: usize,
    pub fork_return_frame: Option<super::arch::ForkReturnFrame>,
    pub kstack: KernelStack,
    pub status: TaskStatus,
    pub exit_code: Option<i32>,
    pub runtime_ticks: u64,
    pub run_start_tick: Option<u64>,
    pub voluntary_switches: u64,
    pub involuntary_switches: u64,
}

impl Task {
    pub fn new(name: String, entry: usize) -> Option<Self> {
        let pid = ProcessId::new();
        Self::new_for_process(name, entry, pid, ProcessId(0))
    }

    pub fn new_for_process(
        name: String,
        entry: usize,
        pid: ProcessId,
        ppid: ProcessId,
    ) -> Option<Self> {
        let id = TaskId::new();
        let kstack = KernelStack::new()?;

        // Initialize context on the kernel stack
        let context_sp = TaskContext::init(entry, kstack.top());

        Some(Self {
            id,
            pid,
            ppid,
            name,
            context_sp,
            fork_return_frame: None,
            kstack,
            status: TaskStatus::Ready,
            exit_code: None,
            runtime_ticks: 0,
            run_start_tick: None,
            voluntary_switches: 0,
            involuntary_switches: 0,
        })
    }

    pub fn new_kernel(name: &str, entry: extern "C" fn()) -> Self {
        Self::new(String::from(name), entry as usize).expect("Failed to create kernel task")
    }

    pub fn new_kernel_for_process(
        name: &str,
        entry: extern "C" fn(),
        pid: ProcessId,
        ppid: ProcessId,
    ) -> Self {
        Self::new_for_process(String::from(name), entry as usize, pid, ppid)
            .expect("Failed to create kernel task")
    }

    #[inline]
    pub fn on_switch_in(&mut self, now_ticks: u64) {
        self.run_start_tick = Some(now_ticks);
    }

    #[inline]
    pub fn on_switch_out(&mut self, now_ticks: u64, involuntary: bool) {
        if let Some(start_tick) = self.run_start_tick.take() {
            self.runtime_ticks = self
                .runtime_ticks
                .saturating_add(now_ticks.saturating_sub(start_tick));
        }

        if involuntary {
            self.involuntary_switches = self.involuntary_switches.saturating_add(1);
        } else {
            self.voluntary_switches = self.voluntary_switches.saturating_add(1);
        }
    }

    #[inline]
    pub fn total_runtime_ticks(&self, now_ticks: u64) -> u64 {
        let running_ticks = self
            .run_start_tick
            .map(|start_tick| now_ticks.saturating_sub(start_tick))
            .unwrap_or(0);
        self.runtime_ticks.saturating_add(running_ticks)
    }
}
