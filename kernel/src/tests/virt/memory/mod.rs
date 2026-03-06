use crate::{kprintln, warn};

pub fn run() {
    kprintln!("[test/virt/memory] step=validate-invalid action=register_region input=size=0 expected=invalid-region actual=running");
    match crate::virt::register_region(0x1000, 0) {
        Err(crate::virt::error::VirtError::InvalidRegion) => {
            kprintln!("[test/virt/memory] step=validate-invalid-result expected=invalid-region actual=invalid-region location=virt::memory::mmio::validate_region");
        }
        Ok(()) => warn!("[test/virt/memory] step=validate-invalid-result expected=invalid-region actual=ok location=virt::memory::mmio::validate_region"),
        Err(err) => warn!("[test/virt/memory] step=validate-invalid-result expected=invalid-region actual={:?} location=virt::memory::mmio::validate_region", err),
    }

    kprintln!("[test/virt/memory] step=register-valid action=register_region input=base=0x1000,size=0x1000 expected=unsupported actual=running");
    match crate::virt::register_region(0x1000, 0x1000) {
        Err(crate::virt::error::VirtError::Unsupported) => {
            kprintln!("[test/virt/memory] step=register-valid-result expected=unsupported actual=unsupported location=virt::memory::mmio::register_region");
        }
        Ok(()) => warn!("[test/virt/memory] step=register-valid-result expected=unsupported actual=ok location=virt::memory::mmio::register_region"),
        Err(err) => warn!("[test/virt/memory] step=register-valid-result expected=unsupported actual={:?} location=virt::memory::mmio::register_region", err),
    }
}
