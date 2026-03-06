use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct TrapDispatchState {
    pub exception_handlers_ready: bool,
    pub syscall_gate_ready: bool,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "interrupt_trap",
    stage: ComponentStage::Core,
    deps: &["interrupt_core"],
    summary: "exception and trap dispatch surface",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
