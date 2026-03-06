use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct InterruptControllerState {
    pub local_apic_ready: bool,
    pub ioapic_ready: bool,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "interrupt_controller",
    stage: ComponentStage::Core,
    deps: &["interrupt_core"],
    summary: "interrupt controller routing and eoi control",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
