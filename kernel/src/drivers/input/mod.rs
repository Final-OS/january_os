//! 输入设备驱动
//!
//! # 支持的输入设备
//!
//! - PS/2 键盘/鼠标
//! - USB HID 键盘/鼠标
//! - ACPI 热键 (TODO)
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                   输入事件层                            │
//! │           (统一的 KeyEvent / MouseEvent)               │
//! ├─────────────────────────────────────────────────────────┤
//! │     PS/2 驱动      │      USB HID 驱动                  │
//! │   (8042 控制器)    │   (xHCI/EHCI/OHCI)                 │
//! └─────────────────────────────────────────────────────────┘
//! ```

pub mod ps2;
pub mod hid;

// 导出 PS/2 键盘接口
pub use ps2::keyboard::{
    init as keyboard_init,
    handle_scancode, read_char, has_char, buffer_len,
    last_scancode, last_char,
    is_shift_pressed, is_ctrl_pressed, is_alt_pressed,
};

// 导出 PS/2 鼠标接口
pub use ps2::mouse::{
    init as mouse_init,
    handle_interrupt as mouse_handle_interrupt,
    left_button, middle_button, right_button,
    delta_x, delta_y,
    event_count as mouse_event_count, has_event as mouse_has_event,
    set_sample_rate, set_resolution,
};

// 导出 USB HID 接口
pub use hid::{
    HidDeviceType, HidManager,
    KeyEvent, KeyCode, Modifiers, KeyEventType,
    MouseEvent, MouseButton, MouseEventType,
};

/// 初始化所有输入设备驱动
pub fn init() {
    // 初始化 USB HID
    let _ = hid::init();

    // 初始化 PS/2 键盘
    keyboard_init();

    // 初始化 PS/2 鼠标
    mouse_init();
}

/// 轮询所有输入设备
pub fn poll() {
    hid::poll();
}
