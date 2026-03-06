use crate::net::device::registry;
use crate::net::device::types::NetDevice;

pub mod e1000;
pub mod loopback;
pub mod virtio;

#[derive(Debug, Clone, Copy)]
pub struct NetDriverReport {
    pub loopback_ready: bool,
    pub virtio_ready: bool,
    pub e1000_ready: bool,
    pub registered_devices: u32,
}

pub fn init() -> NetDriverReport {
    let loopback_ready = registry::register_loopback(loopback::loopback_device());
    let virtio_ready = virtio::probe().is_ok();
    let e1000_ready = e1000::probe().is_ok();

    NetDriverReport {
        loopback_ready,
        virtio_ready,
        e1000_ready,
        registered_devices: registry::registered_devices(),
    }
}

pub fn default_loopback() -> NetDevice {
    loopback::loopback_device()
}
