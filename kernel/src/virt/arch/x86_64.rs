use crate::virt::{HypervisorType, VirtInfo};

#[inline]
fn classify_vendor(vendor: &[u8; 12]) -> HypervisorType {
    match vendor {
        b"KVMKVMKVM\0\0\0" => HypervisorType::Kvm,
        b"Microsoft Hv" => HypervisorType::HyperV,
        b"VMwareVMware" => HypervisorType::Vmware,
        b"XenVMMXenVMM" => HypervisorType::Xen,
        b"TCGTCGTCGTCG" => HypervisorType::Qemu,
        _ => HypervisorType::Unknown,
    }
}

pub fn detect() -> VirtInfo {
    use core::arch::x86_64::__cpuid;

    let leaf1 = unsafe { __cpuid(1) };
    let hypervisor_present = ((leaf1.ecx >> 31) & 1) == 1;
    if !hypervisor_present {
        return VirtInfo::bare_metal();
    }

    let hv_leaf = unsafe { __cpuid(0x4000_0000) };
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&hv_leaf.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&hv_leaf.ecx.to_le_bytes());
    vendor[8..12].copy_from_slice(&hv_leaf.edx.to_le_bytes());

    VirtInfo {
        is_virtualized: true,
        hypervisor: classify_vendor(&vendor),
        vendor_id: vendor,
        nested_supported: false,
    }
}
