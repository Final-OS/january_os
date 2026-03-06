use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub trait ArchIrq {
    fn enable(&self);
    fn disable(&self);
    fn enabled(&self) -> bool;
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "arch_irq",
    stage: ComponentStage::Core,
    deps: &["arch_cpu"],
    summary: "interrupt enablement abstraction",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
