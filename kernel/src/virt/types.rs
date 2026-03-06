pub use super::core::state::VirtState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VcpuId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemSlotId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqRouteId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioRegion {
    pub base: u64,
    pub size: u64,
}
