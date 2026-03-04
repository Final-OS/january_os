//! 输入设备驱动
//!
//! # 支持的输入设备
//!
//! - PS/2 键盘/鼠标
//! - USB HID 键盘/鼠标
//! - ACPI 热键
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

pub mod acpi_hotkey;
pub mod hid;
pub mod ps2;

// 导出 PS/2 键盘接口
pub use ps2::keyboard::{
    buffer_len, handle_scancode, init as keyboard_init, is_alt_pressed, is_ctrl_pressed,
    is_shift_pressed, last_char, last_scancode,
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
    delta_x, delta_y, device_id as mouse_device_id, event_count as mouse_event_count,
    handle_interrupt as mouse_handle_interrupt, has_event as mouse_has_event, init as mouse_init,
    left_button, middle_button, right_button, set_resolution, set_sample_rate,
};

// 导出 USB HID 接口
pub use acpi_hotkey::HotkeyEvent;
pub use hid::{
    HidDeviceType, HidManager, KeyCode, KeyEvent, KeyEventType, Modifiers, MouseButton, MouseEvent,
    MouseEventType,
};

/// 初始化所有输入设备驱动
pub fn init() {
    // 初始化 ACPI 热键
    acpi_hotkey::init();

    // 初始化 USB HID
    let _ = hid::init();

    // 初始化 PS/2 键盘
    keyboard_init();

    // 初始化 PS/2 鼠标
    mouse_init();
}

/// 轮询所有输入设备
pub fn poll() {
    acpi_hotkey::poll();
    hid::poll();
}

/// 读取一个 ACPI 热键事件
pub fn read_hotkey_event() -> Option<HotkeyEvent> {
    acpi_hotkey::read_event()
}

/// 检查是否有 ACPI 热键事件
pub fn has_hotkey_event() -> bool {
    acpi_hotkey::has_event()
}

/// 注入 ACPI 热键事件（测试/调试用）
pub fn inject_hotkey_event(event: HotkeyEvent) {
    acpi_hotkey::push_event(event);
}

/// 获取 ACPI 热键缓冲区状态 (head, tail)
pub fn hotkey_buffer_status() -> (usize, usize) {
    acpi_hotkey::buffer_status()
}

/// 获取 ACPI 热键事件源信息
pub fn hotkey_source_info() -> Option<acpi_hotkey::HotkeySourceInfo> {
    acpi_hotkey::source_info()
}
