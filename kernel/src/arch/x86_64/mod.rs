//! x86_64 架构支持模块
//!
//! 提供 x86_64 特定的硬件抽象。

pub mod boot;
pub mod cpu;
pub mod io;
pub mod power;
pub mod serial;
pub mod syscall;

pub use boot::switch_to_runtime_boot_stack;
pub use cpu::{current_stack_top, halt};
pub use io::{inb, inl, inw, outb, outl, outw};
pub use power::{reboot, shutdown};
pub use serial::{serial_init, serial_print, serial_println};
