use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub trait ContextSwitch {
    fn switch_to(&self, next_stack_top: usize) -> !;
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "arch_context",
    stage: ComponentStage::Core,
    deps: &["arch_cpu", "arch_mmu"],
    summary: "context switch and privilege transition abstraction",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
