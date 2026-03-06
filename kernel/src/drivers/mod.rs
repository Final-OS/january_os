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
//! └── tty/        # 终端设备
//! ```

pub mod acpi;
pub mod block;
pub mod input;
pub mod pci;
pub mod tty;
pub mod usb;

#[derive(Debug, Clone, Copy)]
pub struct DriverInitReport {
    pub block_ready: bool,
    pub pci_ready: bool,
    pub usb_ready: bool,
    pub input_ready: bool,
}

pub fn init_all() -> DriverInitReport {
    block::init();
    pci::init();
    usb::init();
    input::init();

    DriverInitReport {
        block_ready: true,
        pci_ready: true,
        usb_ready: true,
        input_ready: true,
    }
}
