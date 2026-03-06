#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuExitReason {
    Halt,
    Io,
    Mmio,
    Hypercall,
    ExternalInterrupt,
    Unknown,
}
