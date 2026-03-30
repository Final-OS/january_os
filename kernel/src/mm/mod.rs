//! january_os 内存管理子系统
//!
//! 参考 Linux 内核设计，实现完整的物理和虚拟内存管理。

use ::alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub mod alloc;
pub mod api;
pub mod arch;
pub mod boot;
pub mod diag;
pub mod dma;
pub mod phys;
pub mod runtime;
pub mod syscall;
pub mod virt;

pub use alloc::heap;
pub use alloc::slub;
pub use alloc::vmalloc;
pub use api::layout;
pub use boot::setup;
pub use dma as iommu;
pub use phys as page;
pub use virt as vm;
pub use virt::layout_runtime;

pub use phys::buddy;
pub use phys::buddy::{alloc_page, alloc_pages, free_page, free_pages};
pub use phys::memblock;
pub use phys::memblock::{
    memblock_add, memblock_alloc, memblock_alloc_range, memblock_alloc_zeroed, memblock_free,
    memblock_free_size, memblock_init, memblock_initialized, memblock_memory_region,
    memblock_memory_region_count, memblock_phys_mem_size, memblock_reserve,
    memblock_reserved_region, memblock_reserved_region_count, memblock_reserved_size,
};
pub use phys::numa;
pub use phys::numa::{NumaNodeInfo, init_numa, init_uma};
pub use phys::page::{
    MAX_PFN, Page, PageFlags, PageOwner, VMEMMAP_BASE, max_pfn, page_guard_stats, page_to_pfn,
    pfn_to_page, vmemmap_base_ptr,
};
pub use phys::pcp;
pub use phys::pcp::{drain_all_pcps, init_pcp, pcp_initialized, pcp_stats};
pub use phys::physical::{MemoryRegion, MemoryRegionType};
pub use phys::zone;
pub use phys::zone::{
    GFP_DMA32, GFP_KERNEL, GFP_KERNEL_ZERO, GFP_USER, GfpFlags, MAX_ORDER, ZoneType, get_order,
    get_zone,
};

pub use api::layout::{
    DIRECT_MAP_OFFSET, KERNEL_BASE, PAGE_SIZE, USER_MMAP_BASE, USER_SPACE_END, USER_SPACE_START,
    USER_STACK_SIZE, USER_STACK_TOP, is_user_addr, page_align_down, page_align_up, phys_to_virt,
    virt_to_phys,
};
pub use virt::fault;
pub use virt::fault::{FaultContext, FaultResult, get_fault_stats, handle_page_fault};
pub use virt::layout_runtime::{
    boot_reported_page_levels, boot_reported_root_phys, boot_reported_va_bits, direct_map_end,
    direct_map_offset, hardware_page_levels, hardware_root_phys, hardware_va_bits,
    init_from_boot_info, is_vmalloc_addr, page_levels, paging_corrected_by_hw,
    paging_root_mismatch, snapshot, va_bits, vmalloc_end, vmalloc_start,
};
pub use virt::paging;
pub use virt::paging::{pt_reclaim_stats, register_tlb_shootdown_cpu, run_tlb_probe_on_other_cpus};
pub use virt::vma::{
    Mm, VmFlags, VmaInfo, get_init_mm, init_mm_ptr, init_vma, mm_clone, mm_release, mm_retain,
    mmap_flags, mmap_flags_to_vm_flags, prot_flags,
};

pub use arch::{
    PTE_ADDR_MASK, PTE_NO_EXECUTE, PTE_PRESENT, PTE_USER, PTE_WRITABLE, PageTableManager,
    level_index,
};

pub use alloc::heap::{heap_stats, init_heap};
pub use dma::{IommuType, TranslationMode, init_iommu, iommu_stats};

pub use boot::setup::{
    MemoryRegionInfo, MmInitStage, finish_mm_init, init_buddy_system, init_memblock, init_slub,
    init_stage,
};

#[derive(Debug, Clone, Copy)]
pub struct MmComponentReport {
    pub page_levels: u8,
    pub va_bits: u8,
    pub direct_map_start: u64,
    pub direct_map_end: u64,
    pub vmalloc_start: u64,
    pub vmalloc_end: u64,
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "mm",
    stage: ComponentStage::Core,
    deps: &["mm_layout"],
    summary: "memblock, buddy, slub, vm, vmalloc and iommu runtime",
};

pub fn component_report() -> MmComponentReport {
    diag::stats::component_report()
}

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
    diag::stats::component_stats()
}

pub fn dump_state() -> String {
    diag::dump::dump_state()
}
