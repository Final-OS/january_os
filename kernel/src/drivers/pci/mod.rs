//! PCI 驱动
//!
//! 提供 PCI 配置空间访问和设备枚举功能

pub mod msix;
pub mod pcie;
pub mod pci;

pub use pci::*;

pub fn init() {
    pcie::init();
}
