//! USB 键盘驱动
//!
//! 支持 USB HID 键盘设备

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use crate::sync::Once;
use super::hid::{BootKeyboardReport, HidProtocol};

// ============================================================================
// USB HID 键盘码到 ASCII 映射
// ============================================================================

/// USB HID 键盘码 (部分)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyCode {
    None = 0x00,
    ErrorRollOver = 0x01,
    PostFail = 0x02,
    ErrorUndefined = 0x03,
    A = 0x04,
    B = 0x05,
    C = 0x06,
    D = 0x07,
    E = 0x08,
    F = 0x09,
    G = 0x0A,
    H = 0x0B,
    I = 0x0C,
    J = 0x0D,
    K = 0x0E,
    L = 0x0F,
    M = 0x10,
    N = 0x11,
    O = 0x12,
    P = 0x13,
    Q = 0x14,
    R = 0x15,
    S = 0x16,
    T = 0x17,
    U = 0x18,
    V = 0x19,
    W = 0x1A,
    X = 0x1B,
    Y = 0x1C,
    Z = 0x1D,
    Num1 = 0x1E,
    Num2 = 0x1F,
    Num3 = 0x20,
    Num4 = 0x21,
    Num5 = 0x22,
    Num6 = 0x23,
    Num7 = 0x24,
    Num8 = 0x25,
    Num9 = 0x26,
    Num0 = 0x27,
    Enter = 0x28,
    Escape = 0x29,
    Backspace = 0x2A,
    Tab = 0x2B,
    Space = 0x2C,
    Minus = 0x2D,
    Equal = 0x2E,
    LeftBracket = 0x2F,
    RightBracket = 0x30,
    Backslash = 0x31,
    NonUsHash = 0x32,
    Semicolon = 0x33,
    Quote = 0x34,
    Grave = 0x35,
    Comma = 0x36,
    Period = 0x37,
    Slash = 0x38,
    CapsLock = 0x39,
    F1 = 0x3A,
    F2 = 0x3B,
    F3 = 0x3C,
    F4 = 0x3D,
    F5 = 0x3E,
    F6 = 0x3F,
    F7 = 0x40,
    F8 = 0x41,
    F9 = 0x42,
    F10 = 0x43,
    F11 = 0x44,
    F12 = 0x45,
    PrintScreen = 0x46,
    ScrollLock = 0x47,
    Pause = 0x48,
    Insert = 0x49,
    Home = 0x4A,
    PageUp = 0x4B,
    Delete = 0x4C,
    End = 0x4D,
    PageDown = 0x4E,
    RightArrow = 0x4F,
    LeftArrow = 0x50,
    DownArrow = 0x51,
    UpArrow = 0x52,
    NumLock = 0x53,
    // 小键盘
    KpDivide = 0x54,
    KpMultiply = 0x55,
    KpMinus = 0x56,
    KpPlus = 0x57,
    KpEnter = 0x58,
    Kp1 = 0x59,
    Kp2 = 0x5A,
    Kp3 = 0x5B,
    Kp4 = 0x5C,
    Kp5 = 0x5D,
    Kp6 = 0x5E,
    Kp7 = 0x5F,
    Kp8 = 0x60,
    Kp9 = 0x61,
    Kp0 = 0x62,
    KpPeriod = 0x63,
    // 修饰键
    LeftCtrl = 0xE0,
    LeftShift = 0xE1,
    LeftAlt = 0xE2,
    LeftGui = 0xE3,
    RightCtrl = 0xE4,
    RightShift = 0xE5,
    RightAlt = 0xE6,
    RightGui = 0xE7,
}

impl KeyCode {
    /// 从 USB HID 码创建
    pub fn from_hid(code: u8) -> Self {
        // Safety: 我们接受任何值，无效值映射为 None
        unsafe { core::mem::transmute(code) }
    }
    
    /// 转换为 ASCII (无修饰键)
    pub fn to_ascii(&self) -> Option<u8> {
        HID_TO_ASCII.get(*self as usize).copied().filter(|&c| c != 0)
    }
    
    /// 转换为 ASCII (带 Shift)
    pub fn to_ascii_shift(&self) -> Option<u8> {
        HID_TO_ASCII_SHIFT.get(*self as usize).copied().filter(|&c| c != 0)
    }
}

/// HID 键盘码到 ASCII 映射 (无 Shift)
#[rustfmt::skip]
const HID_TO_ASCII: [u8; 128] = [
//  0     1     2     3     4     5     6     7     8     9     A     B     C     D     E     F
    0,    0,    0,    0,  b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l', // 0x0_
  b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z', b'1', b'2', // 0x1_
  b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0',b'\n',  27,    8,b'\t', b' ', b'-', b'=', b'[', // 0x2_
  b']',b'\\',   0, b';',b'\'', b'`', b',', b'.', b'/',   0,    0,    0,    0,    0,    0,    0, // 0x3_
    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,  127,    0,    0,    0, // 0x4_
    0,    0,    0,    0, b'/', b'*', b'-', b'+',b'\n', b'1', b'2', b'3', b'4', b'5', b'6', b'7', // 0x5_
  b'8', b'9', b'0', b'.',   0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0, // 0x6_
    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0, // 0x7_
];

/// HID 键盘码到 ASCII 映射 (带 Shift)
#[rustfmt::skip]
const HID_TO_ASCII_SHIFT: [u8; 128] = [
//  0     1     2     3     4     5     6     7     8     9     A     B     C     D     E     F
    0,    0,    0,    0,  b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L', // 0x0_
  b'M', b'N', b'O', b'P', b'Q', b'R', b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z', b'!', b'@', // 0x1_
  b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')',b'\n',  27,    8,b'\t', b' ', b'_', b'+', b'{', // 0x2_
  b'}', b'|',   0, b':', b'"', b'~', b'<', b'>', b'?',   0,    0,    0,    0,    0,    0,    0, // 0x3_
    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,  127,    0,    0,    0, // 0x4_
    0,    0,    0,    0, b'/', b'*', b'-', b'+',b'\n', b'1', b'2', b'3', b'4', b'5', b'6', b'7', // 0x5_
  b'8', b'9', b'0', b'.',   0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0, // 0x6_
    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0, // 0x7_
];

// ============================================================================
// 修饰键
// ============================================================================

/// 修饰键状态
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub left_ctrl: bool,
    pub left_shift: bool,
    pub left_alt: bool,
    pub left_gui: bool,
    pub right_ctrl: bool,
    pub right_shift: bool,
    pub right_alt: bool,
    pub right_gui: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
}

impl Modifiers {
    /// 从 HID 修饰键字节创建
    pub fn from_hid(byte: u8) -> Self {
        Self {
            left_ctrl: byte & 0x01 != 0,
            left_shift: byte & 0x02 != 0,
            left_alt: byte & 0x04 != 0,
            left_gui: byte & 0x08 != 0,
            right_ctrl: byte & 0x10 != 0,
            right_shift: byte & 0x20 != 0,
            right_alt: byte & 0x40 != 0,
            right_gui: byte & 0x80 != 0,
            caps_lock: false,
            num_lock: false,
            scroll_lock: false,
        }
    }
    
    /// 检查是否按下任意 Shift
    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }
    
    /// 检查是否按下任意 Ctrl
    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }
    
    /// 检查是否按下任意 Alt
    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }
    
    /// 检查是否按下任意 GUI
    pub fn gui(&self) -> bool {
        self.left_gui || self.right_gui
    }
}

// ============================================================================
// 按键事件
// ============================================================================

/// 按键事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventType {
    /// 按下
    Press,
    /// 释放
    Release,
    /// 重复 (长按)
    Repeat,
}

/// 按键事件
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    /// 键码
    pub keycode: KeyCode,
    /// 事件类型
    pub event_type: KeyEventType,
    /// 修饰键状态
    pub modifiers: Modifiers,
    /// ASCII 字符 (如果有)
    pub ascii: Option<u8>,
}

impl KeyEvent {
    /// 创建按键事件
    pub fn new(keycode: KeyCode, event_type: KeyEventType, modifiers: Modifiers) -> Self {
        let ascii = if event_type == KeyEventType::Press || event_type == KeyEventType::Repeat {
            let base = if modifiers.shift() ^ modifiers.caps_lock {
                keycode.to_ascii_shift()
            } else {
                keycode.to_ascii()
            };
            
            // 处理 Ctrl 组合键
            if let Some(c) = base {
                if modifiers.ctrl() && c >= b'a' && c <= b'z' {
                    Some(c - b'a' + 1) // Ctrl+A = 1, Ctrl+B = 2, ...
                } else if modifiers.ctrl() && c >= b'A' && c <= b'Z' {
                    Some(c - b'A' + 1)
                } else {
                    Some(c)
                }
            } else {
                None
            }
        } else {
            None
        };
        
        Self {
            keycode,
            event_type,
            modifiers,
            ascii,
        }
    }
}

// ============================================================================
// USB 键盘设备
// ============================================================================

/// USB 键盘状态
pub struct UsbKeyboard {
    /// USB 设备地址
    pub usb_address: u8,
    /// 接口号
    pub interface: u8,
    /// IN 端点
    pub in_endpoint: u8,
    /// 当前协议
    pub protocol: HidProtocol,
    /// 上一次报告
    pub last_report: BootKeyboardReport,
    /// 修饰键状态
    pub modifiers: Modifiers,
    /// 是否活跃
    pub active: bool,
}

impl UsbKeyboard {
    /// 创建新的 USB 键盘
    pub fn new(usb_address: u8, interface: u8, in_endpoint: u8) -> Self {
        Self {
            usb_address,
            interface,
            in_endpoint,
            protocol: HidProtocol::Boot,
            last_report: BootKeyboardReport::default(),
            modifiers: Modifiers::default(),
            active: false,
        }
    }
    
    /// 处理 Boot 协议报告
    pub fn process_report(&mut self, report: &BootKeyboardReport) {
        // 更新修饰键
        self.modifiers = Modifiers::from_hid(report.modifiers);
        
        // 检测新按下的键
        for &keycode in &report.keycodes {
            if keycode == 0 {
                continue;
            }
            
            // 检查是否是新按下的键
            let was_pressed = self.last_report.keycodes.contains(&keycode);
            
            if !was_pressed {
                // 新按下
                let event = KeyEvent::new(
                    KeyCode::from_hid(keycode),
                    KeyEventType::Press,
                    self.modifiers,
                );
                push_key_event(event);
            }
        }
        
        // 检测释放的键
        for &keycode in &self.last_report.keycodes {
            if keycode == 0 {
                continue;
            }
            
            let still_pressed = report.keycodes.contains(&keycode);
            
            if !still_pressed {
                // 已释放
                let event = KeyEvent::new(
                    KeyCode::from_hid(keycode),
                    KeyEventType::Release,
                    self.modifiers,
                );
                push_key_event(event);
            }
        }
        
        // 保存报告
        self.last_report = *report;
    }
}

// ============================================================================
// 全局状态
// ============================================================================

static mut GLOBAL_KEYBOARD: Option<UsbKeyboard> = None;

/// 处理 Boot 协议报告 (供外部驱动调用)
pub fn handle_boot_report(report: BootKeyboardReport) {
    unsafe {
        if (*core::ptr::addr_of!(GLOBAL_KEYBOARD)).is_none() {
            // 初始化默认实例
            GLOBAL_KEYBOARD = Some(UsbKeyboard::new(0, 0, 0));
        }
        
        if let Some(kbd) = &mut *core::ptr::addr_of_mut!(GLOBAL_KEYBOARD) {
            kbd.process_report(&report);
        }
    }
}

/// 事件缓冲区大小
const EVENT_BUFFER_SIZE: usize = 64;

/// 按键事件缓冲区
static mut KEY_EVENT_BUFFER: [KeyEvent; EVENT_BUFFER_SIZE] = [KeyEvent {
    keycode: KeyCode::None,
    event_type: KeyEventType::Press,
    modifiers: Modifiers {
        left_ctrl: false, left_shift: false, left_alt: false, left_gui: false,
        right_ctrl: false, right_shift: false, right_alt: false, right_gui: false,
        caps_lock: false, num_lock: false, scroll_lock: false,
    },
    ascii: None,
}; EVENT_BUFFER_SIZE];

static KEY_EVENT_HEAD: AtomicUsize = AtomicUsize::new(0);
static KEY_EVENT_TAIL: AtomicUsize = AtomicUsize::new(0);

/// ASCII 字符缓冲区
const CHAR_BUFFER_SIZE: usize = 256;
static CHAR_BUFFER: [AtomicU8; CHAR_BUFFER_SIZE] = {
    const INIT: AtomicU8 = AtomicU8::new(0);
    [INIT; CHAR_BUFFER_SIZE]
};
static CHAR_HEAD: AtomicUsize = AtomicUsize::new(0);
static CHAR_TAIL: AtomicUsize = AtomicUsize::new(0);

static USB_KEYBOARD_INIT: Once = Once::new();

/// 初始化 USB 键盘驱动
pub fn init() {
    USB_KEYBOARD_INIT.call_once(|| {
        // USB 键盘初始化逻辑
    });
}

/// 轮询 USB 键盘
pub fn poll() {
    // TODO: 实际轮询 USB 设备
    // 这需要 USB 主机控制器驱动支持
}

/// 推送按键事件
fn push_key_event(event: KeyEvent) {
    // 1. 推送到事件缓冲区 (如果未满)
    let head = KEY_EVENT_HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % EVENT_BUFFER_SIZE;
    
    if next_head != KEY_EVENT_TAIL.load(Ordering::Relaxed) {
        unsafe {
            // 使用 addr_of_mut! 避免 static_mut_refs
            (*core::ptr::addr_of_mut!(KEY_EVENT_BUFFER))[head] = event;
        }
        KEY_EVENT_HEAD.store(next_head, Ordering::Relaxed);
    }
    
    // 2. 独立推送到字符缓冲区 (即使事件缓冲区已满)
    if let Some(c) = event.ascii {
        push_char(c);
    }
}

/// 推送字符
fn push_char(c: u8) {
    let head = CHAR_HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % CHAR_BUFFER_SIZE;
    
    if next_head != CHAR_TAIL.load(Ordering::Relaxed) {
        CHAR_BUFFER[head].store(c, Ordering::Relaxed);
        CHAR_HEAD.store(next_head, Ordering::Relaxed);
    }
}

/// 读取按键事件
pub fn read_event() -> Option<KeyEvent> {
    let tail = KEY_EVENT_TAIL.load(Ordering::Relaxed);
    let head = KEY_EVENT_HEAD.load(Ordering::Relaxed);
    
    if tail == head {
        return None;
    }
    
    let event = unsafe { KEY_EVENT_BUFFER[tail] };
    KEY_EVENT_TAIL.store((tail + 1) % EVENT_BUFFER_SIZE, Ordering::Relaxed);
    Some(event)
}

/// 读取字符
pub fn read_char() -> Option<u8> {
    let tail = CHAR_TAIL.load(Ordering::Relaxed);
    let head = CHAR_HEAD.load(Ordering::Relaxed);
    
    if tail == head {
        return None;
    }
    
    let c = CHAR_BUFFER[tail].load(Ordering::Relaxed);
    CHAR_TAIL.store((tail + 1) % CHAR_BUFFER_SIZE, Ordering::Relaxed);
    Some(c)
}

/// 检查是否有字符可读
pub fn has_char() -> bool {
    CHAR_TAIL.load(Ordering::Relaxed) != CHAR_HEAD.load(Ordering::Relaxed)
}

/// 检查是否有事件可读
pub fn has_event() -> bool {
    KEY_EVENT_TAIL.load(Ordering::Relaxed) != KEY_EVENT_HEAD.load(Ordering::Relaxed)
}
