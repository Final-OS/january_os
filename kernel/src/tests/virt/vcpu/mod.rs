use crate::{kprintln, warn};

pub fn run() {
    let vcpu_id = crate::virt::VcpuId(0);
    kprintln!("[test/virt/vcpu] step=run action=virt::run_vcpu input={:?} expected=unsupported actual=running", vcpu_id);
    match crate::virt::run_vcpu(vcpu_id) {
        Err(crate::virt::error::VirtError::Unsupported) => {
            kprintln!("[test/virt/vcpu] step=run-result expected=unsupported actual=unsupported location=virt::vcpu::lifecycle::run");
        }
        Ok(()) => warn!("[test/virt/vcpu] step=run-result expected=unsupported actual=ok location=virt::vcpu::lifecycle::run"),
        Err(err) => warn!("[test/virt/vcpu] step=run-result expected=unsupported actual={:?} location=virt::vcpu::lifecycle::run", err),
    }
}
