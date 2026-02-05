//! USB 驱动子系统
//!
//! 支持 xHCI 主机控制器和 USB 设备枚举

pub mod xhci;

use crate::kprintln;

/// 初始化 USB 子系统
pub fn init() {
    kprintln!("USB: Initializing USB subsystem...");
    xhci::init();
}

/// 轮询 USB 事件
pub fn poll() {
    xhci::poll();
}
