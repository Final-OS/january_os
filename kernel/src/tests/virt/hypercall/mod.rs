use crate::{kprintln, warn};

pub fn run() {
    kprintln!(
        "[test/virt/hypercall] step=dispatch action=hypercall::dispatch input=nr0 expected=unsupported actual=running"
    );
    match crate::virt::hypercall::dispatch(0, 0, 0) {
        Err(crate::virt::error::VirtError::Unsupported) => {
            kprintln!(
                "[test/virt/hypercall] step=dispatch-result expected=unsupported actual=unsupported location=virt::hypercall::handlers::handle"
            );
        }
        Ok(ret) => warn!(
            "[test/virt/hypercall] step=dispatch-result expected=unsupported actual=ok({}) location=virt::hypercall::handlers::handle",
            ret
        ),
        Err(err) => warn!(
            "[test/virt/hypercall] step=dispatch-result expected=unsupported actual={:?} location=virt::hypercall::handlers::handle",
            err
        ),
    }
}
