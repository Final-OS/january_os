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
