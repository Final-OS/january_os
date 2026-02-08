//! x86_64 架构支持模块
//!
//! 提供 x86_64 特定的硬件抽象。

pub mod serial;
pub mod cpu;
pub mod power;
pub mod syscall;

pub use serial::{serial_init, serial_print, serial_println};
pub use cpu::{current_stack_top, halt};
pub use power::{shutdown, reboot};
