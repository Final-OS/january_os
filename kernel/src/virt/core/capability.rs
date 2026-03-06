#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtCapability {
    Detection,
    HostControl,
    VmLifecycle,
    VcpuLifecycle,
    MemorySlots,
    Mmio,
    IrqRouting,
    Hypercall,
    DeviceModel,
}
