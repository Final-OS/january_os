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
    let cloned_mm = mm::mm_clone(init_mm);
    if cloned_mm.is_null() {
        mm::vmalloc::iounmap(io);
        return fail("pt_ownership", "mm_clone failed");
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

    unsafe { mm::mm_release(cloned_mm) };

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
