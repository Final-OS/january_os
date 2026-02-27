//! 虚拟化能力探测与抽象
//!
//! 目标：
//! - 提供统一的“当前是否处于虚拟机环境”探测入口
//! - 为后续 KVM/VT-x/ARM VE/RISC-V H 扩展支持预留组件边界

mod arch;

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
}

impl VirtInfo {
    pub const fn bare_metal() -> Self {
        Self {
            is_virtualized: false,
            hypervisor: HypervisorType::None,
            vendor_id: [0; 12],
            nested_supported: false,
        }
    }

    pub fn vendor_str(&self) -> &str {
        let len = self
            .vendor_id
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.vendor_id.len());
        core::str::from_utf8(&self.vendor_id[..len]).unwrap_or("unknown")
    }
}

pub fn detect() -> VirtInfo {
    arch::detect()
}
