//! january_os 设备驱动子系统
//!
//! # 目录结构
//!
//! ```text
//! drivers/
//! ├── acpi/       # ACPI 表解析和电源管理
//! ├── input/      # 输入设备 (键盘、鼠标)
//! │   ├── ps2/    # PS/2 控制器驱动
//! │   └── hid/    # USB HID 驱动 (TODO)
//! └── tty/        # 终端设备
//!     ├── serial  # 串口 TTY
//!     └── console # 控制台
//! ```

pub mod acpi;
pub mod input;
pub mod tty;
