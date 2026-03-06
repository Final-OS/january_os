use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct TrapFrameSummary {
    pub instruction_pointer: usize,
    pub stack_pointer: usize,
    pub error_code: usize,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "arch_trap",
    stage: ComponentStage::Core,
    deps: &["arch_irq"],
    summary: "trap frame and exception entry abstraction",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
