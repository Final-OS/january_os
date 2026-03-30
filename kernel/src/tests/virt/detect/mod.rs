use crate::{kprintln, warn};

pub fn run() {
    kprintln!(
        "[test/virt/detect] step=detect action=call virt::detect expected=info actual=running"
    );
    let info = crate::virt::detect();
    kprintln!(
        "[test/virt/detect] step=detect-result action=inspect vendor expected=string actual={} virtualized={} hypervisor={:?}",
        info.vendor_str(),
        info.is_virtualized,
        info.hypervisor,
    );
    if info.vendor_str().is_empty() {
        warn!(
            "[test/virt/detect] step=detect-result expected=non-empty-vendor actual=empty fallback=allowed-on-bare-metal"
        );
    }
}
