use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "interrupt_core",
    stage: ComponentStage::Core,
    deps: &["interrupt"],
    summary: "interrupt core bookkeeping and status",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
