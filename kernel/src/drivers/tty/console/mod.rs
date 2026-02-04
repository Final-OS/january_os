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

/// Framebuffer 信息
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// 物理/虚拟地址
    pub address: u64,
    /// 宽度 (像素)
    pub width: u32,
    /// 高度 (像素)
    pub height: u32,
    /// 每行字节数
    pub stride: u32,
    /// 每像素位数
    pub bpp: u32,
}

/// Framebuffer 全局状态（一次性初始化）
static FRAMEBUFFER: OnceCell<FramebufferInfo> = OnceCell::new();

/// 初始化 Framebuffer
pub fn init_framebuffer(info: &FramebufferInfo) {
    let _ = FRAMEBUFFER.set(*info);
}

/// 检查 Framebuffer 是否已初始化
pub fn framebuffer_initialized() -> bool {
    FRAMEBUFFER.is_initialized()
}

/// 获取 Framebuffer 信息
pub fn framebuffer_info() -> Option<&'static FramebufferInfo> {
    FRAMEBUFFER.get()
}

/// 获取 Framebuffer 地址
pub fn framebuffer_addr() -> u64 {
    FRAMEBUFFER.get().map(|i| i.address).unwrap_or(0)
}

/// 获取 Framebuffer 宽度
pub fn framebuffer_width() -> u32 {
    FRAMEBUFFER.get().map(|i| i.width).unwrap_or(0)
}

/// 获取 Framebuffer 高度
pub fn framebuffer_height() -> u32 {
    FRAMEBUFFER.get().map(|i| i.height).unwrap_or(0)
}

/// 获取 Framebuffer 步幅
pub fn framebuffer_stride() -> u32 {
    FRAMEBUFFER.get().map(|i| i.stride).unwrap_or(0)
}

/// 获取 Framebuffer 每像素位数
pub fn framebuffer_bpp() -> u32 {
    FRAMEBUFFER.get().map(|i| i.bpp).unwrap_or(32)
}

// ============================================================================
// 字符单元格
// ============================================================================

/// 字符属性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharAttr {
    /// 前景色
    pub fg: u32,
    /// 背景色
    pub bg: u32,
    /// 粗体
    pub bold: bool,
    /// 下划线
    pub underline: bool,
    /// 反显
    pub reverse: bool,
    /// 闪烁
    pub blink: bool,
}

impl Default for CharAttr {
    fn default() -> Self {
        Self {
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            bold: false,
            underline: false,
            reverse: false,
            blink: false,
        }
    }
}

impl CharAttr {
    /// 获取实际前景色 (考虑反显)
    pub fn effective_fg(&self) -> u32 {
        if self.reverse { self.bg } else { self.fg }
    }
    
    /// 获取实际背景色 (考虑反显)
    pub fn effective_bg(&self) -> u32 {
        if self.reverse { self.fg } else { self.bg }
    }
}

/// 字符单元格
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    /// 字符 (Unicode 码点)
    pub ch: char,
    /// 属性
    pub attr: CharAttr,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attr: CharAttr::default(),
        }
    }
}

// ============================================================================
// 控制台
// ============================================================================

/// 控制台状态
pub struct Console {
    /// 控制台编号 (0-5 对应 tty1-tty6)
    pub index: usize,
    /// 列数
    pub cols: usize,
    /// 行数
    pub rows: usize,
    /// 光标列
    pub cursor_x: usize,
    /// 光标行
    pub cursor_y: usize,
    /// 光标可见
    pub cursor_visible: bool,
    /// 当前属性
    pub attr: CharAttr,
    /// VT 解析器
    pub vt_parser: VtParser,
    /// 是否活跃
    pub active: bool,
    /// 屏幕缓冲区 (rows * cols)
    screen: &'static mut [Cell],
    /// 滚动缓冲区
    scrollback: &'static mut [Cell],
    /// 滚动偏移
    scroll_offset: usize,
    /// 滚动缓冲区使用量
    scrollback_used: usize,
}

impl Console {
    /// 在已分配的缓冲区上创建控制台
    /// 
    /// # Safety
    /// 调用者必须确保 screen 和 scrollback 指向有效内存
    pub unsafe fn new(
        index: usize,
        cols: usize,
        rows: usize,
        screen: &'static mut [Cell],
        scrollback: &'static mut [Cell],
    ) -> Self {
        Self {
            index,
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            attr: CharAttr::default(),
            vt_parser: VtParser::new(),
            active: false,
            screen,
            scrollback,
            scroll_offset: 0,
            scrollback_used: 0,
        }
    }
    
    /// 清屏
    pub fn clear(&mut self) {
        for cell in self.screen.iter_mut() {
            *cell = Cell::default();
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }
    
    /// 清除从光标到行尾
    pub fn clear_to_eol(&mut self) {
        let start = self.cursor_y * self.cols + self.cursor_x;
        let end = (self.cursor_y + 1) * self.cols;
        for i in start..end {
            self.screen[i] = Cell::default();
        }
    }
    
    /// 清除从光标到屏幕底部
    pub fn clear_to_eos(&mut self) {
        self.clear_to_eol();
        let start = (self.cursor_y + 1) * self.cols;
        for i in start..self.screen.len() {
            self.screen[i] = Cell::default();
        }
    }
    
    /// 清除从行首到光标
    pub fn clear_to_bol(&mut self) {
        let start = self.cursor_y * self.cols;
        let end = start + self.cursor_x + 1;
        for i in start..end {
            self.screen[i] = Cell::default();
        }
    }
    
    /// 清除当前行
    pub fn clear_line(&mut self) {
        let start = self.cursor_y * self.cols;
        let end = start + self.cols;
        for i in start..end {
            self.screen[i] = Cell::default();
        }
    }
    
    /// 写入字符
    pub fn put_char(&mut self, ch: char) {
        match ch {
            '\n' => {
                self.newline();
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\t' => {
                let next_tab = (self.cursor_x / TAB_WIDTH + 1) * TAB_WIDTH;
                self.cursor_x = next_tab.min(self.cols - 1);
            }
            '\x08' => {
                // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            '\x07' => {
                // Bell - 可以触发蜂鸣或闪烁
            }
            _ => {
                // 可打印字符
                if ch >= ' ' {
                    let idx = self.cursor_y * self.cols + self.cursor_x;
                    if idx < self.screen.len() {
                        self.screen[idx] = Cell {
                            ch,
                            attr: self.attr,
                        };
                    }
                    self.cursor_x += 1;
                    if self.cursor_x >= self.cols {
                        self.newline();
                    }
                }
            }
        }
    }
    
    /// 换行
    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;
        if self.cursor_y >= self.rows {
            self.scroll_up(1);
            self.cursor_y = self.rows - 1;
        }
    }
    
    /// 向上滚动
    pub fn scroll_up(&mut self, lines: usize) {
        if lines == 0 || lines > self.rows {
            return;
        }
        
        // 保存到滚动缓冲区
        let save_lines = lines.min(SCROLLBACK_LINES - self.scrollback_used);
        if save_lines > 0 && !self.scrollback.is_empty() {
            let scrollback_cols = self.cols;
            let src_start = 0;
            let src_end = save_lines * self.cols;
            let dst_start = self.scrollback_used * scrollback_cols;
            
            if dst_start + (src_end - src_start) <= self.scrollback.len() {
                for i in 0..(src_end - src_start) {
                    self.scrollback[dst_start + i] = self.screen[src_start + i];
                }
                self.scrollback_used += save_lines;
            }
        }
        
        // 滚动屏幕
        let scroll_cells = lines * self.cols;
        for i in 0..(self.screen.len() - scroll_cells) {
            self.screen[i] = self.screen[i + scroll_cells];
        }
        
        // 清除底部
        for i in (self.screen.len() - scroll_cells)..self.screen.len() {
            self.screen[i] = Cell::default();
        }
    }
    
    /// 向下滚动
    pub fn scroll_down(&mut self, lines: usize) {
        if lines == 0 || lines > self.rows {
            return;
        }
        
        let scroll_cells = lines * self.cols;
        for i in (scroll_cells..self.screen.len()).rev() {
            self.screen[i] = self.screen[i - scroll_cells];
        }
        
        // 清除顶部
        for i in 0..scroll_cells {
            self.screen[i] = Cell::default();
        }
    }
    
    /// 移动光标
    pub fn move_cursor(&mut self, x: usize, y: usize) {
        self.cursor_x = x.min(self.cols - 1);
        self.cursor_y = y.min(self.rows - 1);
    }
    
    /// 相对移动光标
    pub fn move_cursor_rel(&mut self, dx: isize, dy: isize) {
        let new_x = (self.cursor_x as isize + dx).max(0) as usize;
        let new_y = (self.cursor_y as isize + dy).max(0) as usize;
        self.move_cursor(new_x, new_y);
    }
    
    /// 写入字符串
    pub fn write_str(&mut self, s: &str) {
        for ch in s.chars() {
            // 通过 VT 解析器处理
            for action in self.vt_parser.feed(ch) {
                self.handle_vt_action(action);
            }
        }
    }
    
    /// 处理 VT 动作
    fn handle_vt_action(&mut self, action: VtAction) {
        match action {
            VtAction::Print(ch) => self.put_char(ch),
            VtAction::CursorUp(n) => self.move_cursor_rel(0, -(n as isize)),
            VtAction::CursorDown(n) => self.move_cursor_rel(0, n as isize),
            VtAction::CursorForward(n) => self.move_cursor_rel(n as isize, 0),
            VtAction::CursorBack(n) => self.move_cursor_rel(-(n as isize), 0),
            VtAction::CursorPosition(row, col) => {
                self.move_cursor(col.saturating_sub(1), row.saturating_sub(1));
            }
            VtAction::EraseDisplay(mode) => match mode {
                0 => self.clear_to_eos(),
                1 => {
                    self.clear_to_bol();
                    // 清除光标以上
                    let end = self.cursor_y * self.cols;
                    for i in 0..end {
                        self.screen[i] = Cell::default();
                    }
                }
                2 | 3 => self.clear(),
                _ => {}
            },
            VtAction::EraseLine(mode) => match mode {
                0 => self.clear_to_eol(),
                1 => self.clear_to_bol(),
                2 => self.clear_line(),
                _ => {}
            },
            VtAction::SetAttr(attr) => {
                // 处理 SGR 属性
                match attr {
                    0 => self.attr = CharAttr::default(),
                    1 => self.attr.bold = true,
                    4 => self.attr.underline = true,
                    5 => self.attr.blink = true,
                    7 => self.attr.reverse = true,
                    22 => self.attr.bold = false,
                    24 => self.attr.underline = false,
                    25 => self.attr.blink = false,
                    27 => self.attr.reverse = false,
                    30..=37 => self.attr.fg = ANSI_COLORS[(attr - 30) as usize],
                    38 => {} // 扩展前景色 (需要参数)
                    39 => self.attr.fg = DEFAULT_FG,
                    40..=47 => self.attr.bg = ANSI_COLORS[(attr - 40) as usize],
                    48 => {} // 扩展背景色 (需要参数)
                    49 => self.attr.bg = DEFAULT_BG,
                    90..=97 => self.attr.fg = ANSI_COLORS[(attr - 90 + 8) as usize],
                    100..=107 => self.attr.bg = ANSI_COLORS[(attr - 100 + 8) as usize],
                    _ => {}
                }
            }
            VtAction::ScrollUp(n) => self.scroll_up(n),
            VtAction::ScrollDown(n) => self.scroll_down(n),
            VtAction::SaveCursor => {
                // TODO: 保存光标位置
            }
            VtAction::RestoreCursor => {
                // TODO: 恢复光标位置
            }
            VtAction::ShowCursor => self.cursor_visible = true,
            VtAction::HideCursor => self.cursor_visible = false,
            VtAction::Bell => {
                // TODO: 响铃
            }
            VtAction::Reset => {
                self.clear();
                self.attr = CharAttr::default();
                self.cursor_visible = true;
            }
        }
    }
    
    /// 获取单元格
    pub fn get_cell(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.cols && y < self.rows {
            Some(&self.screen[y * self.cols + x])
        } else {
            None
        }
    }
    
    /// 渲染到 Framebuffer
    pub fn render(&self) {
        if !framebuffer_initialized() {
            return;
        }
        
        let fb_addr = framebuffer_addr() as *mut u32;
        let fb_stride = framebuffer_stride() as usize / 4;
        
        for row in 0..self.rows {
            for col in 0..self.cols {
                let cell = &self.screen[row * self.cols + col];
                self.render_cell(fb_addr, fb_stride, col, row, cell);
            }
        }
        
        // 渲染光标
        if self.cursor_visible {
            self.render_cursor(fb_addr, fb_stride);
        }
    }
    
    /// 渲染单个字符单元格
    fn render_cell(&self, fb: *mut u32, stride: usize, col: usize, row: usize, cell: &Cell) {
        let char_width = font::FONT_WIDTH;
        let char_height = font::FONT_HEIGHT;
        let x = col * char_width;
        let y = row * char_height;
        
        let fg = cell.attr.effective_fg();
        let bg = cell.attr.effective_bg();
        
        let glyph = font::get_glyph(cell.ch);
        
        for dy in 0..char_height {
            for dx in 0..char_width {
                let pixel_x = x + dx;
                let pixel_y = y + dy;
                let offset = pixel_y * stride + pixel_x;
                
                let bit = (glyph[dy] >> (char_width - 1 - dx)) & 1;
                let color = if bit != 0 { fg } else { bg };
                
                unsafe {
                    *fb.add(offset) = color;
                }
            }
        }
    }
    
    /// 渲染光标
    fn render_cursor(&self, fb: *mut u32, stride: usize) {
        let char_width = font::FONT_WIDTH;
        let char_height = font::FONT_HEIGHT;
        let x = self.cursor_x * char_width;
        let y = self.cursor_y * char_height;
        
        // 绘制底部光标条
        let cursor_height = 2;
        for dy in (char_height - cursor_height)..char_height {
            for dx in 0..char_width {
                let pixel_x = x + dx;
                let pixel_y = y + dy;
                let offset = pixel_y * stride + pixel_x;
                
                unsafe {
                    *fb.add(offset) = self.attr.fg;
                }
            }
        }
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

// ============================================================================
// 全局控制台状态
// ============================================================================

static CONSOLE_INIT: Once = Once::new();
static ACTIVE_CONSOLE: AtomicUsize = AtomicUsize::new(0);

/// 初始化控制台子系统
pub fn init() {
    CONSOLE_INIT.call_once(|| {
        // 控制台初始化逻辑
    });
}

/// 检查是否已初始化
pub fn initialized() -> bool {
    CONSOLE_INIT.is_completed()
}

/// 获取活跃控制台编号
pub fn active_console() -> usize {
    ACTIVE_CONSOLE.load(Ordering::Relaxed)
}

/// 切换活跃控制台
pub fn switch_console(index: usize) {
    if index < MAX_CONSOLES {
        ACTIVE_CONSOLE.store(index, Ordering::Relaxed);
    }
}

// ============================================================================
// 控制台写入器 (用于 kprint!)
// ============================================================================

/// 控制台写入器
pub struct ConsoleWriter;

impl Write for ConsoleWriter {
    fn write_str(&mut self, _s: &str) -> fmt::Result {
        // TODO: 写入活跃控制台
        Ok(())
    }
}
