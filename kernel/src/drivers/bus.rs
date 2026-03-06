use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct ProbeContext {
    pub bus_name: &'static str,
    pub enumeration_complete: bool,
}

pub trait Bus {
    fn name(&self) -> &'static str;
    fn probe(&self) -> ProbeContext;
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "drivers_bus",
    stage: ComponentStage::Late,
    deps: &["drivers_base"],
    summary: "bus enumeration and probe routing",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
