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
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static BLOCK_READY: AtomicBool = AtomicBool::new(false);
static CLASS_READY: AtomicBool = AtomicBool::new(false);
static PCI_READY: AtomicBool = AtomicBool::new(false);
static USB_READY: AtomicBool = AtomicBool::new(false);
static INPUT_READY: AtomicBool = AtomicBool::new(false);
static NET_READY: AtomicBool = AtomicBool::new(false);
static NET_DEVICES_REGISTERED: AtomicU32 = AtomicU32::new(0);

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

    let report = DriverInitReport {
        block_ready: true,
        class_ready: true,
        pci_ready: true,
        usb_ready: true,
        input_ready: true,
        net_ready: net_report.loopback_ready,
        net_devices_registered: net_report.registered_devices,
    };
    store_report(report);
    report
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
    if INITIALIZED.load(Ordering::Acquire) {
        ComponentStats::ready()
    } else {
        ComponentStats::registered()
    }
}

pub fn dump_state() -> String {
    let report = snapshot_report();
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

fn store_report(report: DriverInitReport) {
    BLOCK_READY.store(report.block_ready, Ordering::Release);
    CLASS_READY.store(report.class_ready, Ordering::Release);
    PCI_READY.store(report.pci_ready, Ordering::Release);
    USB_READY.store(report.usb_ready, Ordering::Release);
    INPUT_READY.store(report.input_ready, Ordering::Release);
    NET_READY.store(report.net_ready, Ordering::Release);
    NET_DEVICES_REGISTERED.store(report.net_devices_registered, Ordering::Release);
    INITIALIZED.store(true, Ordering::Release);
}

fn snapshot_report() -> DriverInitReport {
    DriverInitReport {
        block_ready: BLOCK_READY.load(Ordering::Acquire),
        class_ready: CLASS_READY.load(Ordering::Acquire),
        pci_ready: PCI_READY.load(Ordering::Acquire),
        usb_ready: USB_READY.load(Ordering::Acquire),
        input_ready: INPUT_READY.load(Ordering::Acquire),
        net_ready: NET_READY.load(Ordering::Acquire),
        net_devices_registered: NET_DEVICES_REGISTERED.load(Ordering::Acquire),
    }
}
