#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuSchedulingPolicy {
    Pinned,
    Fair,
}
