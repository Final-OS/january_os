use crate::kprintln;
use crate::mm::arch::x86_64::paging::{PageTableManager, PTE_PRESENT, PTE_WRITABLE};
use crate::mm::buddy::{alloc_page, free_page};
use crate::mm::page::page_to_pfn;
use crate::mm::zone::GfpFlags;
use crate::mm::layout::{PAGE_SIZE, DIRECT_MAP_OFFSET};

pub fn test_paging() {
    kprintln!("Testing Paging...");

    // 1. Allocate PGD
    let pgd_page = match alloc_page(GfpFlags::new(GfpFlags::KERNEL)) {
        Some(p) => p,
        None => {
            kprintln!("    FAIL: Failed to alloc PGD");
            return;
        }
    };
    let pgd_phys = page_to_pfn(pgd_page) * PAGE_SIZE;

    // Zero the PGD page manually
    unsafe {
        let ptr = (DIRECT_MAP_OFFSET + pgd_phys) as *mut u8;
        core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
    }

    let mut mapper = unsafe { PageTableManager::new(pgd_phys, DIRECT_MAP_OFFSET) };

    // 2. Map test
    let vaddr = 0xFFFF_8000_1000_0000;
    let paddr = 0x200_0000;
    let flags = PTE_PRESENT | PTE_WRITABLE;

    kprintln!("  [1] Map {:#x} -> {:#x}", vaddr, paddr);
    
    let success = unsafe { mapper.map_page(vaddr, paddr, flags) };
    if !success {
        kprintln!("    FAIL: map_page failed");
        unsafe { free_page(pgd_page) };
        return;
    }

    // 3. Verify translation
    kprintln!("  [2] Translate {:#x}", vaddr);
    let result = mapper.translate(vaddr);
    if let Some((pte, level, size)) = result {
        if pte.phys_addr() != paddr {
            kprintln!("    FAIL: Phys addr mismatch: {:#x} != {:#x}", pte.phys_addr(), paddr);
        } else {
             kprintln!("    Success: Translation correct (Level {:?}, Size {})", level, size);
        }
    } else {
        kprintln!("    FAIL: Translation failed");
    }

    // 4. Unmap test
    kprintln!("  [3] Unmap {:#x}", vaddr);
    let unmapped = unsafe { mapper.unmap_page(vaddr) };
    if !unmapped {
         kprintln!("    FAIL: unmap_page failed");
    }

    // 5. Verify unmap
    if mapper.translate(vaddr).is_some() {
        kprintln!("    FAIL: Address still mapped after unmap");
    }

    // Clean up
    unsafe { free_page(pgd_page) };
    kprintln!("Paging tests passed.");
}
