//! january_os 内存管理子系统
//!
//! 参考 Linux 内核设计，实现完整的物理和虚拟内存管理。
//!
//! # 组件概览
//! ... (omitted docs)

use alloc::format;
use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

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
pub mod syscall;
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
pub use page::buddy::{alloc_page, alloc_pages, free_page, free_pages};
pub use page::memblock;
pub use page::memblock::{
    memblock_add, memblock_alloc, memblock_alloc_range, memblock_alloc_zeroed, memblock_free,
    memblock_free_size, memblock_init, memblock_initialized, memblock_memory_region,
    memblock_memory_region_count, memblock_phys_mem_size, memblock_reserve,
    memblock_reserved_region, memblock_reserved_region_count, memblock_reserved_size,
};
pub use page::numa;
pub use page::numa::{init_numa, init_uma, NumaNodeInfo};
pub use page::pcp;
pub use page::pcp::{drain_all_pcps, init_pcp, pcp_initialized, pcp_stats};
pub use page::page::{
    max_pfn, page_guard_stats, page_to_pfn, pfn_to_page, vmemmap_base_ptr, Page, PageFlags,
    PageOwner, MAX_PFN, VMEMMAP_BASE,
};
pub use page::physical::{MemoryRegion, MemoryRegionType};
pub use page::zone;
pub use page::zone::{get_order, get_zone, GfpFlags, ZoneType, GFP_DMA32, GFP_KERNEL, GFP_KERNEL_ZERO, GFP_USER, MAX_ORDER};

// VM 模块导出
pub use vm::fault;
pub use vm::fault::{get_fault_stats, handle_page_fault, FaultContext, FaultResult};
pub use vm::layout;
pub use vm::layout::{
    is_user_addr, page_align_down, page_align_up, phys_to_virt, virt_to_phys, DIRECT_MAP_OFFSET,
    KERNEL_BASE, PAGE_SIZE, USER_MMAP_BASE,
    USER_SPACE_END, USER_SPACE_START, USER_STACK_SIZE, USER_STACK_TOP,
};
pub use vm::layout_runtime;
pub use vm::layout_runtime::{
    boot_reported_page_levels, boot_reported_root_phys, boot_reported_va_bits, direct_map_end,
    direct_map_offset, hardware_page_levels, hardware_root_phys, hardware_va_bits,
    init_from_boot_info, is_vmalloc_addr, page_levels, paging_corrected_by_hw,
    paging_root_mismatch, snapshot, va_bits, vmalloc_end, vmalloc_start,
};
pub use vm::paging;
pub use vm::paging::{pt_reclaim_stats, register_tlb_shootdown_cpu, run_tlb_probe_on_other_cpus};
pub use vm::vma::{get_init_mm, init_mm_ptr, init_vma, mm_clone, mm_release, mm_retain, mmap_flags, mmap_flags_to_vm_flags, prot_flags, Mm, VmaInfo, VmFlags};

// Arch/MMU 导出
pub use arch::{level_index, PageTableManager, PTE_ADDR_MASK, PTE_NO_EXECUTE, PTE_PRESENT, PTE_USER, PTE_WRITABLE};

// 堆/IOMMU 导出
pub use heap::{heap_stats, init_heap};
pub use iommu::{init_iommu, iommu_stats, IommuType, TranslationMode};

#[derive(Debug, Clone, Copy)]
pub struct MmComponentReport {
    pub page_levels: u8,
    pub va_bits: u8,
    pub direct_map_start: u64,
    pub direct_map_end: u64,
    pub vmalloc_start: u64,
    pub vmalloc_end: u64,
}

pub fn component_report() -> MmComponentReport {
    let snapshot = snapshot();
    MmComponentReport {
        page_levels: snapshot.page_levels,
        va_bits: snapshot.va_bits,
        direct_map_start: snapshot.direct_map_start,
        direct_map_end: snapshot.direct_map_end,
        vmalloc_start: snapshot.vmalloc_start,
        vmalloc_end: snapshot.vmalloc_end,
    }
}

// ============================================================================
// 初始化相关
// ============================================================================

pub use setup::{
    finish_mm_init, init_buddy_system, init_memblock, init_slub, init_stage, MemoryRegionInfo,
    MmInitStage,
};

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "mm",
    stage: ComponentStage::Core,
    deps: &["mm_layout"],
    summary: "memblock, buddy, slub, vm, vmalloc and iommu runtime",
};

pub fn init_early() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_core() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_late() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}

pub fn dump_state() -> String {
    let report = component_report();
    format!(
        "component={} state={:?} levels={} va_bits={} direct_map=[{:#x},{:#x}) vmalloc=[{:#x},{:#x})",
        COMPONENT.id,
        stats().state,
        report.page_levels,
        report.va_bits,
        report.direct_map_start,
        report.direct_map_end,
        report.vmalloc_start,
        report.vmalloc_end,
    )
}
