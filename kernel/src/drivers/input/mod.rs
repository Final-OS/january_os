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
    handle_scancode, buffer_len,
    last_scancode, last_char,
    is_shift_pressed, is_ctrl_pressed, is_alt_pressed,
};

/// 读取一个字符 (优先检查 USB 键盘，然后是 PS/2 键盘)
pub fn read_char() -> Option<u8> {
    if let Some(c) = hid::keyboard::read_char() {
        return Some(c);
    }
    ps2::keyboard::read_char()
}

/// 检查是否有字符可读
pub fn has_char() -> bool {
    hid::keyboard::has_char() || ps2::keyboard::has_char()
}

// 导出 PS/2 鼠标接口
pub use ps2::mouse::{
    init as mouse_init,
    handle_interrupt as mouse_handle_interrupt,
    left_button, middle_button, right_button,
    delta_x, delta_y,
    event_count as mouse_event_count, has_event as mouse_has_event,
    set_sample_rate, set_resolution,
    device_id as mouse_device_id,
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
