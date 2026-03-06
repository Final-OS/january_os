//! 引导阶段缓冲区分配

use uefi::boot::{self, MemoryType};

use crate::bootinfo::{
    BootInfo, DiskInfo, KERNEL_STACK_PAGES, MAX_DISKS, MAX_MEMORY_REGIONS, MemoryRegion,
    PAGE_TABLE_BUFFER_PAGES,
};
use crate::paging::PAGE_SIZE;
const LA57_TRAMPOLINE_PAGES: usize = 2;

/// 引导期缓冲区布局（均为物理地址）
#[derive(Clone, Copy)]
pub struct BootBufferLayout {
    pub bootinfo_phys: u64,
    pub memmap_phys: u64,
    pub diskinfo_phys: u64,
    pub cmdline_phys: u64,
    pub page_table_phys: u64,
    pub page_table_aux_phys: u64,
    pub kernel_stack_phys: u64,
    pub la57_trampoline_phys: u64,
}

#[inline]
const fn pages_for(bytes: usize) -> usize {
    (bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize
}

pub fn allocate_boot_buffers() -> BootBufferLayout {
    let bootinfo_pages = pages_for(core::mem::size_of::<BootInfo>());
    let memmap_pages = pages_for(MAX_MEMORY_REGIONS * core::mem::size_of::<MemoryRegion>());
    let diskinfo_pages = pages_for(MAX_DISKS * core::mem::size_of::<DiskInfo>());
    let cmdline_pages = 1usize;

    let bootinfo_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        bootinfo_pages,
    )
    .expect("Failed to allocate BootInfo buffer")
    .as_ptr() as u64;

    let memmap_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        memmap_pages,
    )
    .expect("Failed to allocate memory-map buffer")
    .as_ptr() as u64;

    let diskinfo_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        diskinfo_pages,
    )
    .expect("Failed to allocate disk-info buffer")
    .as_ptr() as u64;

    let cmdline_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        cmdline_pages,
    )
    .expect("Failed to allocate cmdline buffer")
    .as_ptr() as u64;

    let page_table_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        PAGE_TABLE_BUFFER_PAGES,
    )
    .expect("Failed to allocate page-table buffer")
    .as_ptr() as u64;

    // LA57 过渡路径的 5-level root 在 trampoline 32-bit 阶段需要可被低地址寄存器承载，
    // 因此优先把 aux 页表池放在 4GiB 以下；失败时再降级为任意地址。
    let page_table_aux_phys = boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xFFFF_FFFF),
        MemoryType::LOADER_DATA,
        PAGE_TABLE_BUFFER_PAGES,
    )
    .or_else(|_| {
        boot::allocate_pages(
            boot::AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            PAGE_TABLE_BUFFER_PAGES,
        )
    })
    .expect("Failed to allocate aux page-table buffer")
    .as_ptr() as u64;

    let kernel_stack_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        KERNEL_STACK_PAGES,
    )
    .expect("Failed to allocate kernel stack")
    .as_ptr() as u64;

    // LA57 过渡 trampoline 必须位于 4GiB 以下，且需要可执行页面。
    let la57_trampoline_phys = boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xFFFF_FFFF),
        MemoryType::LOADER_CODE,
        LA57_TRAMPOLINE_PAGES,
    )
    .or_else(|_| {
        boot::allocate_pages(
            boot::AllocateType::AnyPages,
            MemoryType::LOADER_CODE,
            LA57_TRAMPOLINE_PAGES,
        )
    })
    .expect("Failed to allocate LA57 trampoline buffer")
    .as_ptr() as u64;

    BootBufferLayout {
        bootinfo_phys,
        memmap_phys,
        diskinfo_phys,
        cmdline_phys,
        page_table_phys,
        page_table_aux_phys,
        kernel_stack_phys,
        la57_trampoline_phys,
    }
}
