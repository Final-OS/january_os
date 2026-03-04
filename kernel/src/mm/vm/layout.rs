//! 内存布局常量
//!
//! 定义内核和用户空间的虚拟地址布局。
//! 大部分常量从 os_cfg.conf 配置文件生成。

use crate::config;

// =============================================================================
// 基本页参数 (从配置导入)
// =============================================================================

/// 页大小 (默认 4 KiB)
pub const PAGE_SIZE: u64 = config::PAGE_SIZE;

/// 页大小位移 (log2(4096) = 12)
pub const PAGE_SHIFT: u64 = config::PAGE_SHIFT;

// =============================================================================
// 物理内存布局
// =============================================================================

/// 内核加载的物理地址 (从配置导入)
pub const KERNEL_PHYS_BASE: u64 = config::KERNEL_PHYS_BASE;

/// 低端内存上限 (保留给 BIOS/VGA 等)
pub const LOW_MEMORY_END: u64 = 0x0010_0000; // 1 MB

// =============================================================================
// 虚拟内存布局 (x86_64 48位规范地址)
// =============================================================================

/// 内核空间起始地址 (高半部分)
/// 所有高于此地址的都是内核空间
pub const KERNEL_BASE: u64 = 0xFFFF_8000_0000_0000;

/// 直接映射区起始地址 (从配置导入)
/// 物理地址 P 映射到虚拟地址 P + DIRECT_MAP_OFFSET
/// 这允许内核直接访问任何物理地址
pub const DIRECT_MAP_OFFSET: u64 = config::DIRECT_MAP_OFFSET;

/// 内核代码/数据的虚拟地址基址
/// 物理地址 KERNEL_PHYS_BASE 映射到此处
pub const KERNEL_TEXT_BASE: u64 = DIRECT_MAP_OFFSET + KERNEL_PHYS_BASE;

/// 内核堆起始虚拟地址
pub const KERNEL_HEAP_BASE: u64 = 0xFFFF_FF00_0000_0000;

/// 内核堆最大大小 (512 GB)
pub const KERNEL_HEAP_MAX_SIZE: u64 = 512 * 1024 * 1024 * 1024;

/// 内核堆初始大小 (从配置导入)
pub const KERNEL_HEAP_INIT_SIZE: u64 = config::KERNEL_HEAP_INIT_SIZE;

/// 内核栈区域起始地址
pub const KERNEL_STACK_BASE: u64 = 0xFFFF_FF80_0000_0000;

/// 每个 CPU 的内核栈大小 (从配置导入)
pub const KERNEL_STACK_SIZE: u64 = config::KERNEL_STACK_SIZE;

/// 栈保护页大小 (防止栈溢出)
pub const STACK_GUARD_SIZE: u64 = PAGE_SIZE;

// =============================================================================
// 用户空间布局
// =============================================================================

/// 用户空间最大地址 (规范地址边界)
pub const USER_SPACE_END: u64 = config::USER_SPACE_END;

/// 用户空间起始地址 (NULL 指针保护区之后)
pub const USER_SPACE_START: u64 = config::USER_SPACE_START;

/// 用户栈顶地址
pub const USER_STACK_TOP: u64 = config::USER_STACK_TOP;

/// 用户栈最大可扩展大小
pub const USER_STACK_SIZE: u64 = config::USER_STACK_SIZE;

/// mmap 默认起始基址
pub const USER_MMAP_BASE: u64 = config::USER_MMAP_BASE;

// =============================================================================
// 地址转换辅助函数
// =============================================================================

/// 物理地址转换为直接映射区的虚拟地址
#[inline]
pub fn phys_to_virt(phys: u64) -> u64 {
    super::layout_runtime::direct_map_phys_to_virt(phys)
}

/// 直接映射区的虚拟地址转换为物理地址
#[inline]
pub fn virt_to_phys(virt: u64) -> u64 {
    super::layout_runtime::direct_map_virt_to_phys(virt).unwrap_or(0)
}

/// 检查地址是否在内核空间
#[inline]
pub const fn is_kernel_addr(addr: u64) -> bool {
    addr >= KERNEL_BASE
}

/// 检查地址是否在用户空间
#[inline]
pub const fn is_user_addr(addr: u64) -> bool {
    addr < USER_SPACE_END
}

/// 向上对齐到页边界
#[inline]
pub const fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

/// 向下对齐到页边界
#[inline]
pub const fn page_align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE - 1)
}

/// 计算地址所需的页数
#[inline]
pub const fn pages_needed(size: u64) -> u64 {
    (size + PAGE_SIZE - 1) / PAGE_SIZE
}
