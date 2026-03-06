use crate::virt::device::bus::VirtBus;

pub trait VirtDevice {
    fn name(&self) -> &'static str;
    fn bus(&self) -> VirtBus;
}
