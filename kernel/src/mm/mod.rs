//! january_os 内存管理子系统
//!
//! 参考 Linux 内核设计，实现完整的物理和虚拟内存管理。
//!
//! # 组件概览
//! ... (omitted docs)

// ============================================================================
// 模块声明
// ============================================================================

// 架构特定代码
pub mod arch;

// 物理页管理
pub mod page;
// 虚拟内存管理
pub mod vm;

// 分配器 (直接放在 mm 根目录下)
pub mod heap;
pub mod slub;
pub mod vmalloc;

// IOMMU
pub mod iommu;

// Setup
pub mod setup;

// ============================================================================
// 导出 (扁平化 API)
// ============================================================================

// Page 模块导出
pub use page::buddy;
pub use page::buddy::*;
pub use page::memblock;
pub use page::memblock::*;
// pub use page::page; // Conflict with `pub mod page`
pub use page::page::*;
pub use page::zone;
pub use page::zone::*;
pub use page::numa;
pub use page::numa::*;
pub use page::pcp;
pub use page::pcp::*;
pub use page::physical;
pub use page::physical::*;

// VM 模块导出
pub use vm::address;
pub use vm::address::*;
pub use vm::paging;
pub use vm::paging::*;
pub use vm::vma;
pub use vm::vma::*;
pub use vm::fault;
pub use vm::fault::*;
pub use vm::layout;
pub use vm::layout::*;

// 堆分配器
pub use heap::init_heap;

// IOMMU
pub use iommu::iommu_stats;
pub use iommu::init_iommu;
pub use iommu::{IommuType, TranslationMode};

// ============================================================================
// 初始化相关
// ============================================================================

pub use setup::{
    MmInitStage, init_stage,
    init_memblock, init_buddy_system, init_slub, finish_mm_init,
    MemoryRegionInfo,
};

// ============================================================================
// Memblock 早期分配器
// ============================================================================

pub use page::memblock::{
    memblock_init, memblock_initialized,
    memblock_add, memblock_reserve, memblock_free,
    memblock_alloc, memblock_alloc_range, memblock_alloc_zeroed,
};
