use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::net::device::types::NetDevice;

static REGISTERED_DEVICES: AtomicU32 = AtomicU32::new(0);
static LOOPBACK_REGISTERED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
pub struct DeviceRegistry {
    pub registered: u32,
}

impl DeviceRegistry {
    pub const fn empty() -> Self {
        Self { registered: 0 }
    }

    pub fn snapshot() -> Self {
        Self {
            registered: registered_devices(),
        }
    }
}

pub fn register_device(_device: NetDevice) -> u32 {
    REGISTERED_DEVICES.fetch_add(1, Ordering::AcqRel) + 1
}

pub fn register_loopback(device: NetDevice) -> bool {
    if LOOPBACK_REGISTERED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let _ = register_device(device);
        true
    } else {
        false
    }
}

pub fn registered_devices() -> u32 {
    REGISTERED_DEVICES.load(Ordering::Acquire)
}
