use super::{fail, mm_step, pass};
use crate::mm;

pub(super) fn run() {
    mm_step("pt_ownership: case=clone_release_keeps_init_kernel_mapping");

    let io = mm::vmalloc::ioremap(0xFEE0_1000, mm::PAGE_SIZE as usize);
    if io.is_null() {
        return fail("pt_ownership", "ioremap setup failed");
    }

    let page_virt = (io as u64) & !(mm::PAGE_SIZE - 1);
    let init_mm = mm::init_mm_ptr();
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "[test/mm][pt_ownership][diag] before clone init_mm={:#x} page_virt={:#x}",
            init_mm as usize,
            page_virt
        );
    }
    let cloned_mm = mm::mm_clone(init_mm);
    if cloned_mm.is_null() {
        mm::vmalloc::iounmap(io);
        return fail("pt_ownership", "mm_clone failed");
    }
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "[test/mm][pt_ownership][diag] clone ok cloned_mm={:#x}",
            cloned_mm as usize
        );
    }

    let direct_map = mm::direct_map_offset();
    let init_pgd = unsafe { (*init_mm).pgd };
    let cloned_pgd = unsafe { (*cloned_mm).pgd };

    let init_phys = unsafe { mm::PageTableManager::new(init_pgd, direct_map) }
        .translate_addr(page_virt)
        .map(|p| p & !(mm::PAGE_SIZE - 1));
    let cloned_phys = unsafe { mm::PageTableManager::new(cloned_pgd, direct_map) }
        .translate_addr(page_virt)
        .map(|p| p & !(mm::PAGE_SIZE - 1));

    if init_phys.is_none() || cloned_phys.is_none() || init_phys != cloned_phys {
        unsafe { mm::mm_release(cloned_mm) };
        mm::vmalloc::iounmap(io);
        return fail(
            "pt_ownership",
            "cloned mm lost kernel mapping or mapped to different physical page",
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("[test/mm][pt_ownership][diag] before mm_release");
    }
    unsafe { mm::mm_release(cloned_mm) };
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("[test/mm][pt_ownership][diag] after mm_release");
    }

    let init_phys_after = unsafe { mm::PageTableManager::new(init_pgd, direct_map) }
        .translate_addr(page_virt)
        .map(|p| p & !(mm::PAGE_SIZE - 1));
    if init_phys_after.is_none() {
        mm::vmalloc::iounmap(io);
        return fail(
            "pt_ownership",
            "init mm kernel mapping disappeared after mm_release",
        );
    }

    mm::vmalloc::iounmap(io);
    pass("pt_ownership");
}
