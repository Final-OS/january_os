use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Registered,
    Probed,
    Online,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceDescriptor {
    pub name: &'static str,
    pub bus: &'static str,
    pub class: &'static str,
}

pub trait Driver {
    fn descriptor(&self) -> DeviceDescriptor;
    fn probe(&self) -> DeviceState;
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "drivers_base",
    stage: ComponentStage::Late,
    deps: &["interrupt"],
    summary: "driver and device base abstractions",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
