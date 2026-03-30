use crate::kprintln;

pub fn run() {
    kprintln!(
        "[test/virt/recovery] step=init-late action=virt::init_late expected=placeholder-state actual=running"
    );
    match crate::virt::init_late() {
        Ok(state) => kprintln!(
            "[test/virt/recovery] step=init-late-result expected=detection-ready actual=detection_ready={} vm_ready={} vcpu_ready={} location=virt::init::init_late",
            state.detection_ready,
            state.vm_ready,
            state.vcpu_ready,
        ),
        Err(err) => kprintln!(
            "[test/virt/recovery] step=init-late-result expected=ok actual=err({:?}) location=virt::init::init_late",
            err,
        ),
    }
}
