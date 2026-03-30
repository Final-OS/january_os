// ============================================================================
// january_os - x86_64 架构特定内存管理代码
// ============================================================================

pub mod paging;
pub mod tlb;

// 重新导出常用类型
pub use paging::{
    PTE_ACCESSED, PTE_ADDR_MASK, PTE_DIRTY, PTE_GLOBAL, PTE_HUGE, PTE_NO_CACHE, PTE_NO_EXECUTE,
    PTE_PRESENT, PTE_USER, PTE_WRITABLE, PTE_WRITE_THROUGH, PageTable, PageTableEntry,
    PageTableLevel, PageTableManager, level_index, page_offset, pd_index, pdpt_index, pml4_index,
    pml5_index, pt_index, sync_kernel_root_entries_from_init,
};

pub use tlb::{
    flush_tlb, flush_tlb_all, flush_tlb_range, paging_hardware_state, read_cr2, read_cr3, read_cr4,
    set_global_pages_enabled, write_cr3,
};
