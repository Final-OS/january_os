// kernel/src/smp/trampoline.rs

/// AP 启动 Trampoline 代码 (16-bit -> 64-bit)
/// Origin: 0x8000
/// Compiled from trampoline.asm by Makefile
pub const TRAMPOLINE_CODE: &[u8] = include_bytes!("trampoline.bin");

/// Trampoline 加载地址
pub const TRAMPOLINE_BASE: u64 = 0x8000;

/// 数据偏移量 (相对于 Page End = 0x9000)
/// Layout at 0x8FC0..0x9000:
///   0x8FC0: GDTR (10 bytes: 2-byte limit + 8-byte base)
///   0x8FD0: IDTR (10 bytes: 2-byte limit + 8-byte base)
///   0x8FE0: ARG  (8 bytes: direct_map_base)
///   0x8FE8: CR3  (8 bytes: page table root)
///   0x8FF0: RSP  (8 bytes: stack top)
///   0x8FF8: ENTRY(8 bytes: ap_entry address)
pub const OFFSET_GDTR: u64 = 64; // 0x9000 - 64 = 0x8FC0
pub const OFFSET_IDTR: u64 = 48; // 0x9000 - 48 = 0x8FD0
pub const OFFSET_ARG: u64 = 32; // 0x9000 - 32 = 0x8FE0
pub const OFFSET_CR3: u64 = 24; // 0x9000 - 24 = 0x8FE8
pub const OFFSET_RSP: u64 = 16; // 0x9000 - 16 = 0x8FF0
pub const OFFSET_ENTRY: u64 = 8; // 0x9000 -  8 = 0x8FF8
