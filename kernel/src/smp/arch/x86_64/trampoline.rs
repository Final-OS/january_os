// kernel/src/smp/trampoline.rs

/// AP 启动 Trampoline 代码 (16-bit -> 64-bit)
/// Origin: 0x8000
/// Compiled from trampoline.asm by Makefile
pub const TRAMPOLINE_CODE: &[u8] = include_bytes!("trampoline.bin");

/// Trampoline 加载地址
pub const TRAMPOLINE_BASE: u64 = 0x8000;

/// 数据偏移量 (相对于 Page End)
pub const OFFSET_ARG: u64 = 32;
pub const OFFSET_CR3: u64 = 24;
pub const OFFSET_RSP: u64 = 16;
pub const OFFSET_ENTRY: u64 = 8;
