#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VcpuContext {
    pub instruction_pointer: u64,
    pub stack_pointer: u64,
}
