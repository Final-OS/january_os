//! january_os 设备驱动子系统
//!
//! # 目录结构
//!
//! ```text
//! drivers/
//! ├── acpi/       # ACPI 表解析和电源管理
//! ├── pci/        # PCI 总线驱动
//! ├── usb/        # USB 驱动子系统
//! ├── input/      # 输入设备 (键盘、鼠标)
//! ├── net/        # 网络设备驱动
//! └── tty/        # 终端设备
//! ```

pub mod acpi;
pub mod base;
pub mod block;
pub mod bus;
pub mod class;
pub mod input;
pub mod net;
pub mod pci;
pub mod tty;
pub mod usb;

use alloc::format;
use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct DriverInitReport {
    pub block_ready: bool,
    pub class_ready: bool,
    pub pci_ready: bool,
    pub usb_ready: bool,
    pub input_ready: bool,
    pub net_ready: bool,
    pub net_devices_registered: u32,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "drivers",
    stage: ComponentStage::Late,
    deps: &["interrupt", "iommu"],
    summary: "device manager, buses, classes and runtime probes",
};

pub fn init_all() -> DriverInitReport {
    block::init();
    pci::init();
    usb::init();
    input::init();
    let net_report = net::init();

    DriverInitReport {
        block_ready: true,
        class_ready: true,
        pci_ready: true,
        usb_ready: true,
        input_ready: true,
        net_ready: net_report.loopback_ready,
        net_devices_registered: net_report.registered_devices,
    }
}

pub fn init_early() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_core() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_late() -> crate::error::KernelResult<()> {
    let _ = init_all();
    Ok(())
}

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}

pub fn dump_state() -> String {
    let report = init_all();
    format!(
        "component={} state={:?} block={} class={} pci={} usb={} input={} net={} devices={}",
        COMPONENT.id,
        stats().state,
        report.block_ready,
        report.class_ready,
        report.pci_ready,
        report.usb_ready,
        report.input_ready,
        report.net_ready,
        report.net_devices_registered,
    )
}
