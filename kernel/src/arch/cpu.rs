use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub trait ArchCpu {
    fn cpu_id(&self) -> usize;
    fn name(&self) -> &'static str;
    fn halt(&self) -> !;
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "arch_cpu",
    stage: ComponentStage::Early,
    deps: &[],
    summary: "cpu identity and halt abstraction",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
