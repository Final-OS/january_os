//! PCI 驱动
//!
//! 提供 PCI 配置空间访问和设备枚举功能

pub mod driver;
pub mod msix;
pub mod pci;
pub mod pcie;

pub use driver::{PciDeviceId, PciDriver, ProbeResult, probe_device, register_driver};
pub use pci::*;

pub fn init() {
    pcie::init();

    // Enumerate devices and match drivers
    let mut device_count = 0usize;
    let mut claimed_count = 0usize;

    pci::scan_bus(&mut |addr, header| {
        device_count += 1;

        // Log all VirtIO devices for debugging
        if header.vendor_id == 0x1AF4 {
            crate::diag!(
                "[PCI][VirtIO] [{:02x}:{:02x}.{:x}] {:04x}:{:04x}",
                addr.bus,
                addr.device,
                addr.function,
                header.vendor_id,
                header.device_id
            );
        }

        if probe_device(addr, &header) {
            claimed_count += 1;
        }
    });

    crate::diag!("[PCI] {} devices, {} claimed", device_count, claimed_count);
}
