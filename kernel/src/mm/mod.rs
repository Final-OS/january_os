//! january_os 内存管理子系统
//!
//! 参考 Linux 内核设计，实现完整的物理和虚拟内存管理。
//!
//! # 组件概览
//!
//! | 组件 | 状态 | 说明 |
//! |------|------|------|
//! | struct page | ✅ | 页帧描述符 |
//! | Zone 管理 | ✅ | DMA / DMA32 / Normal |
//! | Buddy System | ✅ | alloc_pages / free_pages |
//! | SLUB | ✅ | kmalloc / kfree / kzalloc |
//! | VMA | ✅ | 虚拟内存区域管理 |
//! | vmalloc | ✅ | 虚拟连续内存分配 |
//! | 页错误处理 | ✅ | demand paging, COW |
//! | PCP | ✅ | Per-CPU Page Cache |
//! | NUMA | ✅ | 多节点内存支持 |
//! | IOMMU | ✅ | 设备 DMA 映射 |
//!
//! # 架构图
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      用户空间接口                                │
//! │                   (mmap, brk, munmap)                           │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                  VMA 管理 (vm_area_struct)                       │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                     页错误处理器                                  │
//! │           (demand paging / COW / stack growth)                  │
//! ├─────────────────────────────────────────────────────────────────┤
//! │    SLUB 分配器          │         vmalloc                       │
//! │ kmalloc/kfree/kzalloc   │    虚拟连续内存分配                    │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                  PCP (Per-CPU Page Cache)                       │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                      Buddy System                                │
//! │              alloc_pages / free_pages (2^order)                 │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                       Zone 管理                                  │
//! │            ZONE_DMA | ZONE_DMA32 | ZONE_NORMAL                  │
//! ├─────────────────────────────────────────────────────────────────┤
//! │          NUMA 节点管理 (pg_data_t / per-node zones)             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                    struct page 数组                              │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                       IOMMU                                      │
//! │               DMA 地址映射 / 设备隔离                            │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

// ============================================================================
// 模块声明
// ============================================================================

// 架构特定代码
pub mod arch;

// 通用模块
pub mod address;
pub mod buddy;
pub mod fault;
pub mod heap;
pub mod init;
pub mod iommu;
pub mod layout;
pub mod memblock;
pub mod numa;
pub mod page;
pub mod pcp;
pub mod physical;
pub mod slub;
pub mod vma;
pub mod vmalloc;
pub mod zone;

// 保留旧的 paging 模块以兼容
pub mod paging;

// ============================================================================
// 初始化相关
// ============================================================================

pub use init::{
    MmInitStage, init_stage,
    init_memblock, init_buddy_system, init_slub, finish_mm_init,
    MemoryRegionInfo,
};

// ============================================================================
// Memblock 早期分配器
// ============================================================================

pub use memblock::{
    memblock_init, memblock_initialized,
    memblock_add, memblock_reserve, memblock_free,
    memblock_alloc, memblock_alloc_range, memblock_alloc_zeroed,
    memblock_set_bottom_up, memblock_set_current_limit,
    memblock_phys_mem_size, memblock_reserved_size, memblock_free_size,
    memblock_end_of_phys_mem,
    memblock_for_each_free_region,
    memblock_memory_region_count, memblock_reserved_region_count,
    memblock_memory_region, memblock_reserved_region,
    MemblockRegion, MemblockFlags,
};

// ============================================================================
// 地址类型
// ============================================================================

pub use address::{PhysAddr, VirtAddr};
pub use layout::*;

// ============================================================================
// 页帧管理
// ============================================================================

pub use page::{Page, PageFlags, ListHead};
pub use page::{pfn_to_page, page_to_pfn, init_vmemmap, PAGE_STRUCT_SIZE};

// ============================================================================
// Zone 和 Buddy
// ============================================================================

pub use zone::{Zone, ZoneType, GfpFlags, MAX_ORDER, NR_ZONES};
pub use zone::{GFP_KERNEL, GFP_ATOMIC, GFP_USER, GFP_DMA, GFP_DMA32, GFP_KERNEL_ZERO};
pub use zone::{get_zone, pages_per_order, bytes_per_order, get_order};
pub use zone::zones_initialized;

pub use buddy::{alloc_pages, free_pages, alloc_page, free_page};
pub use buddy::init_zone_buddy;

// ============================================================================
// SLUB
// ============================================================================

pub use slub::{kmalloc, kzalloc, kfree, slub_initialized};
pub use slub::KmemCache;

// ============================================================================
// VMA (虚拟内存区域)
// ============================================================================

pub use vma::{Vma, VmFlags, Mm};
pub use vma::{mmap_flags, prot_flags, mmap_flags_to_vm_flags};
pub use vma::{get_init_mm, init_vma};

// ============================================================================
// vmalloc
// ============================================================================

pub use vmalloc::{vmalloc, vzalloc, vfree};
pub use vmalloc::{ioremap, iounmap};
pub use vmalloc::{VMALLOC_START, VMALLOC_END};
pub use vmalloc::{init_vmalloc, vmalloc_initialized, vmalloc_stats};

// ============================================================================
// 页错误处理
// ============================================================================

pub use fault::{handle_page_fault, FaultContext, FaultResult, FaultType};
pub use fault::{PageFaultError, get_fault_stats};

// ============================================================================
// PCP (Per-CPU Page Cache)
// ============================================================================

pub use pcp::{init_pcp, pcp_initialized};
pub use pcp::{pcp_alloc_page, pcp_free_page, drain_all_pcps};
pub use pcp::{pcp_stats, PcpStats};

// ============================================================================
// NUMA
// ============================================================================

pub use numa::{NumaPolicy, PgData, NumaNodeInfo};
pub use numa::{init_numa, init_uma, numa_initialized};
pub use numa::{numa_config_enabled, uma_config_enabled};
pub use numa::{numa_node_id, nr_online_nodes, is_numa, node_online};
pub use numa::{get_node_data, select_node, get_fallback_nodes};
pub use numa::{numa_distance, LOCAL_DISTANCE, REMOTE_DISTANCE};
pub use numa::{numa_stats, NumaStats, MAX_NUMNODES};

// ============================================================================
// IOMMU
// ============================================================================

pub use iommu::{DmaAddr, DmaDirection, IommuType, TranslationMode, SWIOTLB_SIZE};
pub use iommu::{init_iommu, iommu_initialized, iommu_enabled};
pub use iommu::{iommu_config_enabled, iommu_config_auto};
pub use iommu::{dma_alloc_coherent, dma_free_coherent};
pub use iommu::{dma_map_single, dma_unmap_single};
pub use iommu::{dma_sync_single_for_device, dma_sync_single_for_cpu};
pub use iommu::{iommu_stats, IommuStats};
pub use iommu::translation_mode;

// ============================================================================
// 页表
// ============================================================================

pub use paging::{PageTable, PageTableEntry, PageTableManager, PageTableLevel};

// ============================================================================
// 早期堆 (兼容)
// ============================================================================

pub use heap::{init_heap, heap_stats, HEAP_SIZE};

// ============================================================================
// 兼容性
// ============================================================================

pub use physical::{MemoryRegion, MemoryRegionType};
