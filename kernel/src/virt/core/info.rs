use super::capability::VirtCapability;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HypervisorType {
    None,
    Kvm,
    Xen,
    HyperV,
    Vmware,
    Qemu,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtInfo {
    pub is_virtualized: bool,
    pub hypervisor: HypervisorType,
    pub vendor_id: [u8; 12],
    pub nested_supported: bool,
    pub host_capabilities: &'static [VirtCapability],
}

impl VirtInfo {
    pub const fn bare_metal() -> Self {
        Self {
            is_virtualized: false,
            hypervisor: HypervisorType::None,
            vendor_id: [0; 12],
            nested_supported: false,
            host_capabilities: &[VirtCapability::Detection],
        }
    }

    pub const fn placeholder_host(hypervisor: HypervisorType, vendor_id: [u8; 12]) -> Self {
        Self {
            is_virtualized: match hypervisor {
                HypervisorType::None => false,
                _ => true,
            },
            hypervisor,
            vendor_id,
            nested_supported: false,
            host_capabilities: &[
                VirtCapability::Detection,
                VirtCapability::HostControl,
                VirtCapability::VmLifecycle,
                VirtCapability::VcpuLifecycle,
                VirtCapability::MemorySlots,
                VirtCapability::Mmio,
                VirtCapability::IrqRouting,
                VirtCapability::Hypercall,
                VirtCapability::DeviceModel,
            ],
        }
    }

    pub fn vendor_str(&self) -> &str {
        let len = self
            .vendor_id
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.vendor_id.len());
        ::core::str::from_utf8(&self.vendor_id[..len]).unwrap_or("unknown")
    }
}
