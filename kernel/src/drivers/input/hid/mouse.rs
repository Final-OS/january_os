//! USB 鼠标驱动
//!
//! 支持 USB HID 鼠标设备

use core::sync::atomic::{AtomicI32, AtomicU8, AtomicUsize, Ordering};
use crate::sync::Once;
use super::hid::{BootMouseReport, HidProtocol};

// ============================================================================
// 鼠标按钮
// ============================================================================

/// 鼠标按钮
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MouseButton {
    /// 左键
    Left = 0,
    /// 右键
    Right = 1,
    /// 中键 (滚轮)
    Middle = 2,
    /// 侧键 4
    Button4 = 3,
    /// 侧键 5
    Button5 = 4,
}

// ============================================================================
// 鼠标事件
// ============================================================================

/// 鼠标事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventType {
    /// 移动
    Move,
    /// 按下
    ButtonDown,
    /// 释放
    ButtonUp,
    /// 滚轮
    Scroll,
}

/// 鼠标事件
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// 事件类型
    pub event_type: MouseEventType,
    /// X 轴位移
    pub dx: i32,
    /// Y 轴位移
    pub dy: i32,
    /// 滚轮位移
    pub scroll: i32,
    /// 按钮 (仅按钮事件)
    pub button: Option<MouseButton>,
    /// 当前按钮状态
    pub buttons: u8,
}

impl MouseEvent {
    /// 创建移动事件
    pub fn move_event(dx: i32, dy: i32, buttons: u8) -> Self {
        Self {
            event_type: MouseEventType::Move,
            dx,
            dy,
            scroll: 0,
            button: None,
            buttons,
        }
    }
    
    /// 创建按钮按下事件
    pub fn button_down(button: MouseButton, buttons: u8) -> Self {
        Self {
            event_type: MouseEventType::ButtonDown,
            dx: 0,
            dy: 0,
            scroll: 0,
            button: Some(button),
            buttons,
        }
    }
    
    /// 创建按钮释放事件
    pub fn button_up(button: MouseButton, buttons: u8) -> Self {
        Self {
            event_type: MouseEventType::ButtonUp,
            dx: 0,
            dy: 0,
            scroll: 0,
            button: Some(button),
            buttons,
        }
    }
    
    /// 创建滚轮事件
    pub fn scroll_event(scroll: i32, buttons: u8) -> Self {
        Self {
            event_type: MouseEventType::Scroll,
            dx: 0,
            dy: 0,
            scroll,
            button: None,
            buttons,
        }
    }
}

// ============================================================================
// USB 鼠标设备
// ============================================================================

/// USB 鼠标状态
pub struct UsbMouse {
    /// USB 设备地址
    pub usb_address: u8,
    /// 接口号
    pub interface: u8,
    /// IN 端点
    pub in_endpoint: u8,
    /// 当前协议
    pub protocol: HidProtocol,
    /// 上一次按钮状态
    pub last_buttons: u8,
    /// 是否活跃
    pub active: bool,
}

impl UsbMouse {
    /// 创建新的 USB 鼠标
    pub fn new(usb_address: u8, interface: u8, in_endpoint: u8) -> Self {
        Self {
            usb_address,
            interface,
            in_endpoint,
            protocol: HidProtocol::Boot,
            last_buttons: 0,
            active: false,
        }
    }
    
    /// 处理 Boot 协议报告
    pub fn process_report(&mut self, report: &BootMouseReport) {
        let buttons = report.buttons;
        
        // 检测按钮变化
        let changed = buttons ^ self.last_buttons;
        
        // 左键
        if changed & 0x01 != 0 {
            if buttons & 0x01 != 0 {
                push_mouse_event(MouseEvent::button_down(MouseButton::Left, buttons));
            } else {
                push_mouse_event(MouseEvent::button_up(MouseButton::Left, buttons));
            }
        }
        
        // 右键
        if changed & 0x02 != 0 {
            if buttons & 0x02 != 0 {
                push_mouse_event(MouseEvent::button_down(MouseButton::Right, buttons));
            } else {
                push_mouse_event(MouseEvent::button_up(MouseButton::Right, buttons));
            }
        }
        
        // 中键
        if changed & 0x04 != 0 {
            if buttons & 0x04 != 0 {
                push_mouse_event(MouseEvent::button_down(MouseButton::Middle, buttons));
            } else {
                push_mouse_event(MouseEvent::button_up(MouseButton::Middle, buttons));
            }
        }
        
        // 移动
        if report.x != 0 || report.y != 0 {
            push_mouse_event(MouseEvent::move_event(
                report.x as i32,
                report.y as i32,
                buttons,
            ));
            
            // 更新累积位置
            MOUSE_X.fetch_add(report.x as i32, Ordering::Relaxed);
            MOUSE_Y.fetch_add(report.y as i32, Ordering::Relaxed);
        }
        
        // 滚轮
        if report.wheel != 0 {
            push_mouse_event(MouseEvent::scroll_event(report.wheel as i32, buttons));
        }
        
        self.last_buttons = buttons;
    }
}

// ============================================================================
// 全局状态
// ============================================================================

/// 事件缓冲区大小
const EVENT_BUFFER_SIZE: usize = 64;

/// 鼠标事件缓冲区
static mut MOUSE_EVENT_BUFFER: [MouseEvent; EVENT_BUFFER_SIZE] = [MouseEvent {
    event_type: MouseEventType::Move,
    dx: 0,
    dy: 0,
    scroll: 0,
    button: None,
    buttons: 0,
}; EVENT_BUFFER_SIZE];

static MOUSE_EVENT_HEAD: AtomicUsize = AtomicUsize::new(0);
static MOUSE_EVENT_TAIL: AtomicUsize = AtomicUsize::new(0);

/// 鼠标累积位置
static MOUSE_X: AtomicI32 = AtomicI32::new(0);
static MOUSE_Y: AtomicI32 = AtomicI32::new(0);

/// 当前按钮状态
static MOUSE_BUTTONS: AtomicU8 = AtomicU8::new(0);

static USB_MOUSE_INIT: Once = Once::new();

/// 初始化 USB 鼠标驱动
pub fn init() {
    USB_MOUSE_INIT.call_once(|| {
        // USB 鼠标初始化逻辑
    });
}

/// 轮询 USB 鼠标
pub fn poll() {
    // TODO: 实际轮询 USB 设备
}

/// 推送鼠标事件
fn push_mouse_event(event: MouseEvent) {
    let head = MOUSE_EVENT_HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % EVENT_BUFFER_SIZE;
    
    if next_head != MOUSE_EVENT_TAIL.load(Ordering::Relaxed) {
        unsafe {
            MOUSE_EVENT_BUFFER[head] = event;
        }
        MOUSE_EVENT_HEAD.store(next_head, Ordering::Relaxed);
        
        // 更新按钮状态
        MOUSE_BUTTONS.store(event.buttons, Ordering::Relaxed);
    }
}

/// 读取鼠标事件
pub fn read_event() -> Option<MouseEvent> {
    let tail = MOUSE_EVENT_TAIL.load(Ordering::Relaxed);
    let head = MOUSE_EVENT_HEAD.load(Ordering::Relaxed);
    
    if tail == head {
        return None;
    }
    
    let event = unsafe { MOUSE_EVENT_BUFFER[tail] };
    MOUSE_EVENT_TAIL.store((tail + 1) % EVENT_BUFFER_SIZE, Ordering::Relaxed);
    Some(event)
}

/// 检查是否有事件可读
pub fn has_event() -> bool {
    MOUSE_EVENT_TAIL.load(Ordering::Relaxed) != MOUSE_EVENT_HEAD.load(Ordering::Relaxed)
}

/// 获取当前鼠标位置
pub fn position() -> (i32, i32) {
    (
        MOUSE_X.load(Ordering::Relaxed),
        MOUSE_Y.load(Ordering::Relaxed),
    )
}

/// 设置鼠标位置
pub fn set_position(x: i32, y: i32) {
    MOUSE_X.store(x, Ordering::Relaxed);
    MOUSE_Y.store(y, Ordering::Relaxed);
}

/// 获取当前按钮状态
pub fn buttons() -> u8 {
    MOUSE_BUTTONS.load(Ordering::Relaxed)
}

/// 检查左键是否按下
pub fn left_button() -> bool {
    MOUSE_BUTTONS.load(Ordering::Relaxed) & 0x01 != 0
}

/// 检查右键是否按下
pub fn right_button() -> bool {
    MOUSE_BUTTONS.load(Ordering::Relaxed) & 0x02 != 0
}

/// 检查中键是否按下
pub fn middle_button() -> bool {
    MOUSE_BUTTONS.load(Ordering::Relaxed) & 0x04 != 0
}
