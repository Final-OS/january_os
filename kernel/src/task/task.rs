use alloc::alloc::{alloc, dealloc, Layout};
use alloc::string::String;
use core::ptr::NonNull;
use super::id::TaskId;
use super::arch::TaskContext;

const KERNEL_STACK_SIZE: usize = 32 * 1024; // 32KB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
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
    pub name: String,
    pub context_sp: usize,
    pub kstack: KernelStack,
    pub status: TaskStatus,
}

impl Task {
    pub fn new(name: String, entry: usize) -> Option<Self> {
        let id = TaskId::new();
        let kstack = KernelStack::new()?;
        
        // Initialize context on the kernel stack
        let context_sp = TaskContext::init(entry, kstack.top());
        
        Some(Self {
            id,
            name,
            context_sp,
            kstack,
            status: TaskStatus::Ready,
        })
    }
    
    pub fn new_kernel(name: &str, entry: extern "C" fn()) -> Self {
        Self::new(String::from(name), entry as usize).expect("Failed to create kernel task")
    }
}
