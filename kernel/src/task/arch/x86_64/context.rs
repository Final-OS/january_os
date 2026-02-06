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

impl TaskContext {
    /// Create a new context for a kernel thread
    /// 
    /// # Arguments
    /// * `entry` - Entry point address
    /// * `kstack_top` - Top of the kernel stack (high address)
    /// 
    /// # Returns
    /// The initial stack pointer value (sp) that points to this context
    pub fn init(entry: usize, kstack_top: usize) -> usize {
        let mut sp = kstack_top;
        
        // Reserve space for TaskContext
        sp -= core::mem::size_of::<TaskContext>();
        
        // Initialize context at that location
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
