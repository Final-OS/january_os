//! TTY 子系统
//!
//! 提供终端设备抽象：
//!
//! # 模块结构
//!
//! ```text
//! drivers/tty/
//! ├── serial/     - 串口终端 (COM1-COM4)
//! ├── console/    - Framebuffer 控制台 (tty1-tty6)
//! └── pty/        - 伪终端 (ptmx, pts/N)
//! ```
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     用户空间                                    │
//! │         ┌─────────┐  ┌─────────┐  ┌─────────┐                  │
//! │         │  Shell  │  │   App   │  │  xterm  │                  │
//! │         └────┬────┘  └────┬────┘  └────┬────┘                  │
//! └──────────────┼────────────┼────────────┼───────────────────────┘
//!                │            │            │
//! ┌──────────────┼────────────┼────────────┼───────────────────────┐
//! │              ▼            ▼            ▼           内核空间    │
//! │         ┌─────────────────────────────────────┐               │
//! │         │           TTY 核心层                │               │
//! │         │      (行规程, 作业控制)              │               │
//! │         └─────────────────────────────────────┘               │
//! │              │            │            │                      │
//! │         ┌────┴───┐  ┌────┴───┐  ┌────┴───┐                   │
//! │         │ Serial │  │Console │  │  PTY   │                   │
//! │         │ttyS0-3 │  │ tty1-6 │  │pts/0-N │                   │
//! │         └────────┘  └────────┘  └────────┘                   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod console;
pub mod fbcon;
pub mod pty;
pub mod serial;

#[derive(Debug, Clone, Copy)]
pub struct TtyInitReport {
    pub serial_ready: bool,
    pub framebuffer_console_ready: bool,
    pub pty_ready: bool,
}

// 导出串口接口
pub use serial::{
    COM1_PORT, Serial, SerialWriter, serial_enable_rx_interrupt, serial_has_input,
    serial_interrupt_handler, serial_read, serial_read_char, serial_try_read, serial_write,
};

// 导出控制台接口
pub use console::{
    ANSI_COLORS, DEFAULT_BG, DEFAULT_FG, MAX_CONSOLES, VtAction, VtParser, VtState, ansi_to_rgb,
};

// 导出 PTY 接口
pub use pty::{
    ControlChar, InputFlags, LocalFlags, MAX_PTYS, OutputFlags, PTY_BUFFER_SIZE, PtyManager,
    PtyPair, RingBuffer, Termios, WinSize,
};

/// 初始化 TTY 子系统
pub fn init() {
    serial::init();
    // console 由 main.rs 中的 fbcon::init() 初始化
    pty::init();
}

pub fn init_early_serial() {
    serial::init();
}

pub fn enable_serial_rx() {
    serial::serial_enable_rx_interrupt();
}

pub fn init_framebuffer_console(
    addr: u64,
    width: u32,
    height: u32,
    stride: u32,
    pixel_format: u32,
) -> bool {
    fbcon::init(addr, width, height, stride, pixel_format);
    fbcon::is_initialized()
}

pub fn init_runtime() -> TtyInitReport {
    serial::init();
    pty::init();

    TtyInitReport {
        serial_ready: true,
        framebuffer_console_ready: fbcon::is_initialized(),
        pty_ready: true,
    }
}
