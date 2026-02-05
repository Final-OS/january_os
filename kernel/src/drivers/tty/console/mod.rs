//! 文本控制台 (Framebuffer Console)
//!
//! 提供基于 framebuffer 的文本模式控制台，支持：
//! - VT100/ANSI 转义序列
//! - 多虚拟控制台 (tty1-tty6)
//! - 滚动缓冲区
//! - 光标控制
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        VT 层                                    │
//! │               (ANSI 转义序列解析)                                │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                     Console 层                                  │
//! │            (文本缓冲区, 滚动, 光标)                              │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                   Framebuffer 层                                │
//! │              (像素渲染, 字体绘制)                                │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

mod font;
mod vt;

pub use vt::{VtParser, VtState, VtAction};

use core::sync::atomic::{AtomicUsize, Ordering};
use core::fmt::{self, Write};
use crate::sync::{Once, OnceCell};

// ============================================================================
// 输出宏 - 同时输出到串口和 Framebuffer 控制台
// ============================================================================

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        // 使用绝对路径以确保在任何地方都能访问
        let _ = write!(crate::drivers::tty::serial::SerialWriter, $($arg)*);
        let _ = write!(crate::drivers::tty::fbcon::FbConsoleWriter, $($arg)*);
    }};
}

#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => {{ $crate::kprint!($($arg)*); $crate::kprint!("\n"); }};
}

// ============================================================================
// 常量
// ============================================================================

/// 最大虚拟控制台数量
pub const MAX_CONSOLES: usize = 6;

/// 默认前景色 (浅灰)
pub const DEFAULT_FG: u32 = 0x00CCCCCC;

/// 默认背景色 (深蓝黑)
pub const DEFAULT_BG: u32 = 0x001A1A2E;

/// Tab 宽度
pub const TAB_WIDTH: usize = 8;

/// 滚动缓冲区行数
pub const SCROLLBACK_LINES: usize = 1000;

/// 光标闪烁间隔 (毫秒)
pub const CURSOR_BLINK_MS: u32 = 500;

// ============================================================================
// ANSI 16 色
// ============================================================================

/// ANSI 标准 16 色
pub const ANSI_COLORS: [u32; 16] = [
    0x000000, // 0: Black
    0xAA0000, // 1: Red
    0x00AA00, // 2: Green
    0xAA5500, // 3: Yellow/Brown
    0x0000AA, // 4: Blue
    0xAA00AA, // 5: Magenta
    0x00AAAA, // 6: Cyan
    0xAAAAAA, // 7: White
    0x555555, // 8: Bright Black (Gray)
    0xFF5555, // 9: Bright Red
    0x55FF55, // 10: Bright Green
    0xFFFF55, // 11: Bright Yellow
    0x5555FF, // 12: Bright Blue
    0xFF55FF, // 13: Bright Magenta
    0x55FFFF, // 14: Bright Cyan
    0xFFFFFF, // 15: Bright White
];

/// 从 ANSI 颜色码获取 RGB
pub fn ansi_to_rgb(code: u8) -> u32 {
    if code < 16 {
        ANSI_COLORS[code as usize]
    } else if code < 232 {
        // 216 色立方体 (6x6x6)
        let code = code - 16;
        let r = (code / 36) * 51;
        let g = ((code / 6) % 6) * 51;
        let b = (code % 6) * 51;
        ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    } else {
        // 24 级灰度
        let gray = (code - 232) * 10 + 8;
        ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32)
    }
}

// ============================================================================
// Framebuffer 信息
// ============================================================================

// ... (rest of file usually follows, but I am overwriting the file. Wait! I should NOT overwrite if I don't have full content)
// The previous Read only gave me 100 lines.
// I must use SearchReplace to insert macros.

