//! 引导页表与内存映射转换

use uefi::boot::MemoryType;

use crate::bootinfo::{KERNEL_PHYS_ADDR, MAX_MEMORY_REGIONS, MemoryRegion, MemoryRegionType};

/// 页大小 4KB
pub const PAGE_SIZE: u64 = 4096;
/// 页表项标志：存在
const PTE_PRESENT: u64 = 1 << 0;
/// 页表项标志：可写
const PTE_WRITABLE: u64 = 1 << 1;
/// 页表项标志：大页 (2MB/1GB)
const PTE_HUGE: u64 = 1 << 7;
/// 页表项标志：全局
const PTE_GLOBAL: u64 = 1 << 8;

/// 页表分配器状态
struct PageTableAllocator {
    next_page: u64,
}

impl PageTableAllocator {
    fn new(start: u64) -> Self {
        Self { next_page: start }
    }

    /// 分配一个零初始化的页面
    unsafe fn alloc_page(&mut self) -> u64 {
        let page = self.next_page;
        self.next_page += PAGE_SIZE;
        unsafe { core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize) };
        page
    }
}

/// 设置内核页表
///
/// 创建以下映射：
/// 1. 恒等映射: 前 4GB 物理地址 = 虚拟地址 (用于引导过渡)
/// 2. 内核映射: 0xFFFF_8000_0010_0000 -> 0x100000 (内核代码)
/// 3. 直接映射: 0xFFFF_8800_0000_0000 + phys -> phys (所有物理内存)
///
/// 返回 PML4 物理地址
pub unsafe fn setup_page_tables(
    kernel_size: u64,
    max_phys_addr: u64,
    page_table_start: u64,
) -> u64 {
    let mut allocator = PageTableAllocator::new(page_table_start);
    unsafe {
        let pml4 = allocator.alloc_page();
        let pml4_table = pml4 as *mut u64;

        // 1. 恒等映射前 4GB
        let pdpt_identity = allocator.alloc_page();
        *pml4_table.add(0) = pdpt_identity | PTE_PRESENT | PTE_WRITABLE;

        let pdpt_identity_table = pdpt_identity as *mut u64;
        for i in 0..4u64 {
            *pdpt_identity_table.add(i as usize) =
                (i * 0x4000_0000) | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE;
        }

        // 2. 内核高半部分映射
        let pml4_index_kernel = 256usize;

        let pdpt_kernel = allocator.alloc_page();
        *pml4_table.add(pml4_index_kernel) = pdpt_kernel | PTE_PRESENT | PTE_WRITABLE;

        let pdpt_kernel_table = pdpt_kernel as *mut u64;

        let pd_kernel = allocator.alloc_page();
        *pdpt_kernel_table.add(0) = pd_kernel | PTE_PRESENT | PTE_WRITABLE;

        let pd_kernel_table = pd_kernel as *mut u64;

        let pt_kernel = allocator.alloc_page();
        *pd_kernel_table.add(0) = pt_kernel | PTE_PRESENT | PTE_WRITABLE;

        let pt_kernel_table = pt_kernel as *mut u64;

        let kernel_pages = (kernel_size + PAGE_SIZE - 1) / PAGE_SIZE;
        let kernel_start_pt_index = (KERNEL_PHYS_ADDR / PAGE_SIZE) as usize;

        for i in 0..kernel_pages as usize {
            let phys_addr = KERNEL_PHYS_ADDR + (i as u64) * PAGE_SIZE;
            *pt_kernel_table.add(kernel_start_pt_index + i) =
                phys_addr | PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL;
        }

        for i in 0..512usize {
            if *pt_kernel_table.add(i) == 0 {
                let phys_addr = (i as u64) * PAGE_SIZE;
                *pt_kernel_table.add(i) = phys_addr | PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL;
            }
        }

        // 3. 直接映射区
        let pml4_index_direct = 272usize;

        let pdpt_direct = allocator.alloc_page();
        *pml4_table.add(pml4_index_direct) = pdpt_direct | PTE_PRESENT | PTE_WRITABLE;

        let pdpt_direct_table = pdpt_direct as *mut u64;

        let max_to_map = max_phys_addr.max(4 * 1024 * 1024 * 1024);
        let gb_pages_needed = (max_to_map + 0x4000_0000 - 1) / 0x4000_0000;
        let gb_pages = gb_pages_needed.min(512);

        for i in 0..gb_pages {
            *pdpt_direct_table.add(i as usize) =
                (i * 0x4000_0000) | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | PTE_GLOBAL;
        }

        pml4
    }
}

pub unsafe fn copy_memory_map<'a>(
    mmap: impl Iterator<Item = &'a uefi::mem::memory_map::MemoryDescriptor>,
    memmap_phys: u64,
) -> (u32, u64, u64, u64) {
    let dest = memmap_phys as *mut MemoryRegion;
    let mut count = 0u32;
    let mut total_mem = 0u64;
    let mut usable_mem = 0u64;
    let mut max_phys_addr = 0u64;

    for entry in mmap {
        if count >= MAX_MEMORY_REGIONS as u32 {
            break;
        }

        let pages = entry.page_count;
        let size = pages * 4096;
        total_mem += size;

        let end_addr = entry.phys_start + size;
        if end_addr > max_phys_addr {
            max_phys_addr = end_addr;
        }

        let region_type = match entry.ty {
            MemoryType::CONVENTIONAL => {
                usable_mem += size;
                MemoryRegionType::Usable
            }
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => {
                MemoryRegionType::BootloaderReclaimable
            }
            MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => {
                usable_mem += size;
                MemoryRegionType::Usable
            }
            MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => {
                MemoryRegionType::Reserved
            }
            MemoryType::ACPI_RECLAIM => MemoryRegionType::AcpiReclaimable,
            MemoryType::ACPI_NON_VOLATILE => MemoryRegionType::AcpiNvs,
            MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => MemoryRegionType::Mmio,
            _ => MemoryRegionType::Reserved,
        };

        let region = MemoryRegion {
            phys_start: entry.phys_start,
            virt_start: entry.virt_start,
            page_count: pages,
            region_type: region_type as u32,
            attributes: entry.att.bits() as u32,
        };

        unsafe { core::ptr::write_volatile(dest.add(count as usize), region) };
        count += 1;
    }

    (count, total_mem, usable_mem, max_phys_addr)
}
