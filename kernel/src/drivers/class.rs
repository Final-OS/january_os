use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct DeviceClass {
    pub name: &'static str,
    pub hotpluggable: bool,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "drivers_class",
    stage: ComponentStage::Late,
    deps: &["drivers_base"],
    summary: "device class registration and lookup",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
