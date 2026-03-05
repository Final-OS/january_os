//! PCI Driver Model
//!
//! Provides driver registration and device matching infrastructure.

use super::pci::{PciAddress, PciHeader};
use crate::diag;
use crate::sync::Mutex;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// PCI device ID matching criteria
pub struct PciDeviceId {
    pub vendor: Option<u16>,
    pub device: Option<u16>,
    pub class_code: Option<u8>,
    pub subclass: Option<u8>,
}

impl PciDeviceId {
    /// Match by vendor and device ID
    pub const fn vendor_device(vendor: u16, device: u16) -> Self {
        Self {
            vendor: Some(vendor),
            device: Some(device),
            class_code: None,
            subclass: None,
        }
    }

    /// Match by class code and subclass
    pub const fn class_subclass(class: u8, subclass: u8) -> Self {
        Self {
            vendor: None,
            device: None,
            class_code: Some(class),
            subclass: Some(subclass),
        }
    }

    /// Check if this ID matches the given header
    pub fn matches(&self, header: &PciHeader) -> bool {
        if let Some(v) = self.vendor {
            if header.vendor_id != v {
                return false;
            }
        }
        if let Some(d) = self.device {
            if header.device_id != d {
                return false;
            }
        }
        if let Some(c) = self.class_code {
            if header.class_code != c {
                return false;
            }
        }
        if let Some(s) = self.subclass {
            if header.subclass != s {
                return false;
            }
        }
        true
    }
}

/// Result of driver probe
pub enum ProbeResult {
    /// Device claimed and initialized successfully
    Claimed,
    /// Device not supported by this driver
    Unsupported,
    /// Error during initialization
    Error(&'static str),
}

/// PCI driver trait
pub trait PciDriver: Send + Sync {
    /// Driver name
    fn name(&self) -> &'static str;

    /// Device IDs this driver supports
    fn supported_ids(&self) -> &[PciDeviceId];

    /// Probe and initialize device
    fn probe(&self, addr: PciAddress, header: &PciHeader) -> ProbeResult;
}

/// Global driver registry
static DRIVER_REGISTRY: Mutex<Vec<Box<dyn PciDriver>>> = Mutex::new(Vec::new());

/// Register a PCI driver
pub fn register_driver(driver: Box<dyn PciDriver>) {
    diag!("[PCI] register driver '{}'", driver.name());
    DRIVER_REGISTRY.lock().push(driver);
}

/// Probe all registered drivers against a device
/// Returns true if any driver claimed the device
pub fn probe_device(addr: PciAddress, header: &PciHeader) -> bool {
    let registry = DRIVER_REGISTRY.lock();

    for driver in registry.iter() {
        for id in driver.supported_ids() {
            if id.matches(header) {
                diag!(
                    "[PCI] [{:02x}:{:02x}.{:x}] {:04x}:{:04x} -> '{}'",
                    addr.bus,
                    addr.device,
                    addr.function,
                    header.vendor_id,
                    header.device_id,
                    driver.name()
                );

                match driver.probe(addr, header) {
                    ProbeResult::Claimed => return true,
                    ProbeResult::Unsupported => continue,
                    ProbeResult::Error(msg) => {
                        crate::error!("[PCI] '{}' probe failed: {}", driver.name(), msg);
                        continue;
                    }
                }
            }
        }
    }

    false
}
