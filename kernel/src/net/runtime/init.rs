use crate::net::device::registry;
use crate::net::error::{NetError, NetResult};
use crate::net::types::NetState;

pub fn init_early() -> NetResult<()> {
    Ok(())
}

pub fn init_core() -> NetResult<()> {
    if registry::registered_devices() == 0 {
        return Err(NetError::DeviceUnavailable);
    }
    Ok(())
}

pub fn init_late() -> NetResult<NetState> {
    let devices_ready = registry::registered_devices() > 0;
    if !devices_ready {
        return Err(NetError::DeviceUnavailable);
    }
    Err(NetError::Unsupported)
}
