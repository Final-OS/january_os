//! 文本控制台 (Framebuffer Console)
//!
//! 提供基于 framebuffer 的文本模式控制台，支持：
//! - VT100/ANSI 转义序列
//! - 多虚拟控制台 (tty1-tty6)
//! - 滚动缓冲区
//! - 光标控制

mod font;
mod vt;

pub use vt::{VtParser, VtState, VtAction};

use core::sync::atomic::{AtomicUsize, Ordering};
use core::fmt::{self, Write};
use crate::sync::IrqMutex;

// ============================================================================
// Console Lock
// ============================================================================

pub struct Console {
    vt_parser: VtParser,
}

impl Console {
    pub const fn new() -> Self {
        Self {
            vt_parser: VtParser::new(),
        }
    }

    /// 处理 VT 动作
    fn process_action(&mut self, action: VtAction) {
        use crate::drivers::tty::fbcon;
        
        match action {
            VtAction::Print(ch) => {
                fbcon::put_char(ch);
            }
            VtAction::CursorUp(n) => {
                let (x, y) = fbcon::get_cursor_pos();
                fbcon::set_cursor_pos(x, y.saturating_sub(n));
            }
            VtAction::CursorDown(n) => {
                let (x, y) = fbcon::get_cursor_pos();
                let (_, rows) = fbcon::get_screen_size();
                let new_y = core::cmp::min(rows.saturating_sub(1), y + n);
                fbcon::set_cursor_pos(x, new_y);
            }
            VtAction::CursorForward(n) => {
                let (x, y) = fbcon::get_cursor_pos();
                let (cols, _) = fbcon::get_screen_size();
                let new_x = core::cmp::min(cols.saturating_sub(1), x + n);
                fbcon::set_cursor_pos(new_x, y);
            }
            VtAction::CursorBack(n) => {
                let (x, y) = fbcon::get_cursor_pos();
                fbcon::set_cursor_pos(x.saturating_sub(n), y);
            }
            VtAction::CursorPosition(row, col) => {
                // ANSI 是 1-based，我们是 0-based
                let r = row.saturating_sub(1);
                let c = col.saturating_sub(1);
                fbcon::set_cursor_pos(c, r);
            }
            VtAction::EraseDisplay(mode) => {
                if mode == 2 {
                    fbcon::clear_screen();
                }
            }
            VtAction::SetAttr(attr) => {
                self.handle_sgr(attr);
            }
            VtAction::Reset => {
                fbcon::set_fg_color(DEFAULT_FG);
                fbcon::set_bg_color(DEFAULT_BG);
                fbcon::clear_screen();
                self.vt_parser.reset();
            }
            _ => {}
        }
    }

    /// 处理 SGR 属性
    fn handle_sgr(&mut self, attr: u8) {
        use crate::drivers::tty::fbcon;
        
        match attr {
            0 => { // Reset
                fbcon::set_fg_color(DEFAULT_FG);
                fbcon::set_bg_color(DEFAULT_BG);
            }
            1 => { // Bold (Bright) - 简化处理：如果是深色则变亮
                let (fg, _) = fbcon::get_colors();
                // TODO: 更好的加粗处理
            }
            30..=37 => { // FG Color
                let color = ansi_to_rgb(attr - 30);
                fbcon::set_fg_color(color);
            }
            38 => {
                // TODO: Extended FG (next params)
            }
            39 => { // Default FG
                fbcon::set_fg_color(DEFAULT_FG);
            }
            40..=47 => { // BG Color
                let color = ansi_to_rgb(attr - 40);
                fbcon::set_bg_color(color);
            }
            48 => {
                // TODO: Extended BG (next params)
            }
            49 => { // Default BG
                fbcon::set_bg_color(DEFAULT_BG);
            }
            90..=97 => { // Bright FG
                let color = ansi_to_rgb(attr - 90 + 8);
                fbcon::set_fg_color(color);
            }
            100..=107 => { // Bright BG
                let color = ansi_to_rgb(attr - 100 + 8);
                fbcon::set_bg_color(color);
            }
            _ => {}
        }
    }
}

/// 全局控制台锁，确保多核输出不乱序，且中断安全
pub static CONSOLE: IrqMutex<Console> = IrqMutex::new(Console::new());

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // 串口输出 (Raw)
        let _ = write!(crate::drivers::tty::serial::SerialWriter, "{}", s);
        
        // Framebuffer 输出 (VT 处理)
        for ch in s.chars() {
            let mut iter = self.vt_parser.feed(ch);
            while let Some(action) = iter.next() {
                self.process_action(action);
            }
        }
        Ok(())
    }
}

// ============================================================================
// 输出宏 - 同时输出到串口和 Framebuffer 控制台
// ============================================================================

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        // 使用绝对路径以确保在任何地方都能访问
        let mut console = $crate::drivers::tty::console::CONSOLE.lock();
        let _ = write!(console, $($arg)*);
    }};
}

#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut console = $crate::drivers::tty::console::CONSOLE.lock();
        let _ = write!(console, $($arg)*);
        let _ = write!(console, "\n");
    }};
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
