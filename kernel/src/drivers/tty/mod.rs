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

pub mod serial;
pub mod console;
pub mod pty;
pub mod fbcon;

// 导出串口接口
pub use serial::{
    Serial, COM1_PORT,
    serial_write, serial_read,
    serial_enable_rx_interrupt, serial_interrupt_handler,
    serial_read_char, serial_has_input, serial_try_read,
    SerialWriter,
};

// 导出控制台接口
pub use console::{
    Console, Cell, CharAttr,
    FramebufferInfo, init_framebuffer, framebuffer_initialized,
    VtParser, VtState, VtAction,
    ANSI_COLORS, ansi_to_rgb,
    MAX_CONSOLES, DEFAULT_FG, DEFAULT_BG,
};

// 导出 PTY 接口
pub use pty::{
    PtyPair, PtyManager, RingBuffer,
    Termios, WinSize, InputFlags, OutputFlags, LocalFlags, ControlChar,
    MAX_PTYS, PTY_BUFFER_SIZE,
};

/// 初始化 TTY 子系统
pub fn init() {
    serial::init();
    console::init();
    pty::init();
}
