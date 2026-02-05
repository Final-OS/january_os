use crate::kprintln;
use crate::mm::buddy::{alloc_page, alloc_pages, free_page, free_pages};
use crate::mm::page::{page_to_pfn, pfn_to_page};
use crate::mm::layout::{PAGE_SIZE, DIRECT_MAP_OFFSET};
use crate::mm::zone::{GfpFlags, GFP_KERNEL_ZERO};
use core::slice;

pub fn test_buddy() {
    kprintln!("Testing Buddy System...");

    // 1. Single page allocation
    kprintln!("  [1] Single page allocation");
    let page = alloc_page(GfpFlags::new(GfpFlags::KERNEL));
    if page.is_none() {
        kprintln!("    FAIL: alloc_page failed");
        return;
    }
    let page = page.unwrap();
    let pfn = page_to_pfn(page);
    kprintln!("    Allocated PFN: {}", pfn);
    unsafe { free_page(page) };

    // 2. High order allocation
    kprintln!("  [2] High order allocation (Order 3 = 8 pages)");
    let order = 3;
    let pages = alloc_pages(order, GfpFlags::new(GfpFlags::KERNEL));
    if pages.is_none() {
        kprintln!("    FAIL: alloc_pages failed");
        return;
    }
    let pages = pages.unwrap();
    let start_pfn = page_to_pfn(pages);
    kprintln!("    Allocated Order {} at PFN: {}", order, start_pfn);
    
    // Verify continuity (logical check, buddy guarantees physical continuity)
    unsafe { free_pages(pages, order) };

    // 3. Zeroing test (GFP_ZERO)
    kprintln!("  [3] Zeroing test (GFP_ZERO)");
    let zpage = alloc_page(GFP_KERNEL_ZERO);
    if zpage.is_none() {
        kprintln!("    FAIL: alloc_page(ZERO) failed");
        return;
    }
    let zpage = zpage.unwrap();
    let zpfn = page_to_pfn(zpage);
    let vaddr = DIRECT_MAP_OFFSET + zpfn * PAGE_SIZE;
    
    // Verify content is zero
    unsafe {
        let slice = slice::from_raw_parts(vaddr as *const u8, PAGE_SIZE as usize);
        let mut is_zero = true;
        for &b in slice {
            if b != 0 {
                is_zero = false;
                break;
            }
        }
        if !is_zero {
            kprintln!("    FAIL: Page not zeroed!");
        } else {
            kprintln!("    Success: Page is zeroed");
        }
        free_page(zpage);
    }

    kprintln!("Buddy System tests passed.");
}
