use crate::net::config;
use crate::net::device::types::NetDevice;

pub const fn loopback_device() -> NetDevice {
    NetDevice::up("lo", config::DEFAULT_LOOPBACK_MTU)
}
