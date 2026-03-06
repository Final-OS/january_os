use crate::virt::device::bus::VirtBus;
use crate::virt::device::model::VirtDevice;

pub struct VirtioBlockDevice;

impl VirtDevice for VirtioBlockDevice {
    fn name(&self) -> &'static str {
        "virtio-blk-model"
    }

    fn bus(&self) -> VirtBus {
        VirtBus::Mmio
    }
}
