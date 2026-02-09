//! 引导阶段缓冲区分配

use uefi::boot::{self, MemoryType};

use crate::bootinfo::{
    BootInfo, DiskInfo, MemoryRegion, KERNEL_STACK_PAGES, MAX_DISKS, MAX_MEMORY_REGIONS,
    PAGE_TABLE_BUFFER_PAGES,
};
use crate::paging::PAGE_SIZE;

/// 引导期缓冲区布局（均为物理地址）
#[derive(Clone, Copy)]
pub struct BootBufferLayout {
    pub bootinfo_phys: u64,
    pub memmap_phys: u64,
    pub diskinfo_phys: u64,
    pub cmdline_phys: u64,
    pub page_table_phys: u64,
    pub kernel_stack_phys: u64,
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

    let kernel_stack_phys = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        KERNEL_STACK_PAGES,
    )
    .expect("Failed to allocate kernel stack")
    .as_ptr() as u64;

    BootBufferLayout {
        bootinfo_phys,
        memmap_phys,
        diskinfo_phys,
        cmdline_phys,
        page_table_phys,
        kernel_stack_phys,
    }
}
