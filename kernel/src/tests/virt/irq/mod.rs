use crate::{kprintln, warn};

pub fn run() {
    kprintln!("[test/virt/irq] step=inject-invalid action=inject_irq input=31 expected=invalid-irq-route actual=running");
    match crate::virt::inject_irq(31) {
        Err(crate::virt::error::VirtError::InvalidIrqRoute) => {
            kprintln!("[test/virt/irq] step=inject-invalid-result expected=invalid-irq-route actual=invalid-irq-route location=virt::irq::inject::inject_irq");
        }
        Ok(()) => warn!("[test/virt/irq] step=inject-invalid-result expected=invalid-irq-route actual=ok location=virt::irq::inject::inject_irq"),
        Err(err) => warn!("[test/virt/irq] step=inject-invalid-result expected=invalid-irq-route actual={:?} location=virt::irq::inject::inject_irq", err),
    }

    kprintln!("[test/virt/irq] step=inject-valid action=inject_irq input=48 expected=unsupported actual=running");
    match crate::virt::inject_irq(48) {
        Err(crate::virt::error::VirtError::Unsupported) => {
            kprintln!("[test/virt/irq] step=inject-valid-result expected=unsupported actual=unsupported location=virt::irq::inject::inject_irq");
        }
        Ok(()) => warn!("[test/virt/irq] step=inject-valid-result expected=unsupported actual=ok location=virt::irq::inject::inject_irq"),
        Err(err) => warn!("[test/virt/irq] step=inject-valid-result expected=unsupported actual={:?} location=virt::irq::inject::inject_irq", err),
    }
}
