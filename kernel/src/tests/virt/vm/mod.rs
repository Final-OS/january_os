use crate::{kprintln, warn};

pub fn run() {
    kprintln!("[test/virt/vm] step=create action=virt::create_vm expected=unsupported actual=running");
    match crate::virt::create_vm() {
        Err(crate::virt::error::VirtError::Unsupported) => {
            kprintln!("[test/virt/vm] step=create-result expected=unsupported actual=unsupported location=virt::vm::lifecycle::create_vm");
        }
        Ok(vm_id) => warn!("[test/virt/vm] step=create-result expected=unsupported actual=ok({:?}) location=virt::vm::lifecycle::create_vm", vm_id),
        Err(err) => warn!("[test/virt/vm] step=create-result expected=unsupported actual={:?} location=virt::vm::lifecycle::create_vm", err),
    }
}
