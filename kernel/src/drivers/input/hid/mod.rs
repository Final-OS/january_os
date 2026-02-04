//! USB HID (Human Interface Device) 驱动
//!
//! 支持 USB 键盘、鼠标等 HID 设备
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    HID 设备层                           │
//! │              (keyboard, mouse, gamepad)                │
//! ├─────────────────────────────────────────────────────────┤
//! │                    HID 核心层                           │
//! │            (报告解析, 描述符解析)                        │
//! ├─────────────────────────────────────────────────────────┤
//! │                    USB 核心层                           │
//! │          (设备枚举, 端点管理, 传输)                      │
//! ├─────────────────────────────────────────────────────────┤
//! │                  主机控制器驱动                          │
//! │              (xHCI / EHCI / OHCI)                       │
//! └─────────────────────────────────────────────────────────┘
//! ```

pub mod usb;
pub mod hid;
pub mod keyboard;
pub mod mouse;

use core::sync::atomic::{AtomicUsize, Ordering};
use crate::sync::Once;

pub use usb::{UsbDevice, UsbEndpoint, UsbTransferType, UsbDirection};
pub use hid::{HidDevice, HidReport, HidReportType, HidDescriptor};
pub use keyboard::{UsbKeyboard, KeyEvent, KeyCode, Modifiers, KeyEventType};
pub use mouse::{UsbMouse, MouseEvent, MouseButton, MouseEventType};

// ============================================================================
// 常量
// ============================================================================

/// 最大 HID 设备数量
pub const MAX_HID_DEVICES: usize = 16;

/// HID 轮询间隔 (毫秒)
pub const HID_POLL_INTERVAL_MS: u32 = 10;

// ============================================================================
// HID 子系统状态
// ============================================================================

static HID_INIT: Once = Once::new();
static HID_DEVICE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// HID 设备管理器
pub struct HidManager {
    /// 已注册的 HID 设备
    devices: [Option<HidDeviceEntry>; MAX_HID_DEVICES],
    /// 设备数量
    count: usize,
}

/// HID 设备条目
struct HidDeviceEntry {
    /// 设备类型
    device_type: HidDeviceType,
    /// USB 设备地址
    usb_address: u8,
    /// 接口号
    interface: u8,
    /// 是否活跃
    active: bool,
}

/// HID 设备类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidDeviceType {
    /// 键盘
    Keyboard,
    /// 鼠标
    Mouse,
    /// 游戏手柄
    Gamepad,
    /// 其他 HID 设备
    Generic,
}

impl HidManager {
    /// 创建新的 HID 管理器
    pub const fn new() -> Self {
        Self {
            devices: [None, None, None, None, None, None, None, None,
                      None, None, None, None, None, None, None, None],
            count: 0,
        }
    }
    
    /// 注册 HID 设备
    pub fn register_device(&mut self, device_type: HidDeviceType, usb_address: u8, interface: u8) -> Option<usize> {
        for (i, slot) in self.devices.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(HidDeviceEntry {
                    device_type,
                    usb_address,
                    interface,
                    active: true,
                });
                self.count += 1;
                HID_DEVICE_COUNT.store(self.count, Ordering::Relaxed);
                return Some(i);
            }
        }
        None
    }
    
    /// 注销 HID 设备
    pub fn unregister_device(&mut self, index: usize) {
        if index < MAX_HID_DEVICES {
            if self.devices[index].is_some() {
                self.devices[index] = None;
                self.count = self.count.saturating_sub(1);
                HID_DEVICE_COUNT.store(self.count, Ordering::Relaxed);
            }
        }
    }
    
    /// 获取设备数量
    pub fn device_count(&self) -> usize {
        self.count
    }
}

// ============================================================================
// 全局 HID 管理器
// ============================================================================

static mut HID_MANAGER: HidManager = HidManager::new();

/// 初始化 HID 子系统
pub fn init() -> Result<(), &'static str> {
    if HID_INIT.is_completed() {
        return Err("HID already initialized");
    }
    
    HID_INIT.call_once(|| {
        // 初始化 USB 键盘驱动
        keyboard::init();
        
        // 初始化 USB 鼠标驱动
        mouse::init();
    });
    
    Ok(())
}

/// 检查 HID 是否已初始化
pub fn initialized() -> bool {
    HID_INIT.is_completed()
}

/// 获取 HID 设备数量
pub fn device_count() -> usize {
    HID_DEVICE_COUNT.load(Ordering::Relaxed)
}

/// 轮询所有 HID 设备
pub fn poll() {
    // 轮询键盘
    keyboard::poll();
    
    // 轮询鼠标
    mouse::poll();
}

/// 注册 HID 设备
pub fn register_device(device_type: HidDeviceType, usb_address: u8, interface: u8) -> Option<usize> {
    let mgr = unsafe { &mut *core::ptr::addr_of_mut!(HID_MANAGER) };
    mgr.register_device(device_type, usb_address, interface)
}

/// 注销 HID 设备
pub fn unregister_device(index: usize) {
    let mgr = unsafe { &mut *core::ptr::addr_of_mut!(HID_MANAGER) };
    mgr.unregister_device(index);
}
