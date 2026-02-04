// ============================================================================
// january_os - x86_64 架构特定内存管理代码
// ============================================================================

pub mod paging;
pub mod tlb;

// 重新导出常用类型
pub use paging::{
    PageTable, PageTableEntry, PageTableLevel, PageTableManager,
    PTE_PRESENT, PTE_WRITABLE, PTE_USER, PTE_WRITE_THROUGH,
    PTE_NO_CACHE, PTE_ACCESSED, PTE_DIRTY, PTE_HUGE,
    PTE_GLOBAL, PTE_NO_EXECUTE, PTE_ADDR_MASK,
    pml4_index, pdpt_index, pd_index, pt_index, page_offset,
};

pub use tlb::{
    flush_tlb, flush_tlb_all, flush_tlb_range,
    read_cr3, write_cr3, read_cr2,
    set_global_pages_enabled,
};
