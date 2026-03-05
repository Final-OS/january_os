use super::{fail, mm_step, pass};
use crate::mm;

pub(super) fn run() {
    mm_step("vmalloc_heal: case=metadata_rebuild_after_manual_unmap");

    let io = mm::vmalloc::ioremap(0xFEE0_2000, mm::PAGE_SIZE as usize);
    if io.is_null() {
        return fail("vmalloc_heal", "ioremap setup failed");
    }

    let page_virt = (io as u64) & !(mm::PAGE_SIZE - 1);
    let direct_map = mm::direct_map_offset();
    let init_root = unsafe { (*mm::init_mm_ptr()).pgd };
    let pt_mgr = unsafe { mm::PageTableManager::new(init_root, direct_map) };

    let before_phys = pt_mgr
        .translate_addr(page_virt)
        .map(|p| p & !(mm::PAGE_SIZE - 1));
    if before_phys.is_none() {
        mm::vmalloc::iounmap(io);
        return fail(
            "vmalloc_heal",
            "setup mapping missing before heal regression",
        );
    }

    if unsafe { !pt_mgr.unmap_page(page_virt) } {
        mm::vmalloc::iounmap(io);
        return fail("vmalloc_heal", "manual unmap for regression setup failed");
    }

    if pt_mgr.translate_addr(page_virt).is_some() {
        mm::vmalloc::iounmap(io);
        return fail("vmalloc_heal", "manual unmap did not clear mapping");
    }

    if !mm::vmalloc::ensure_vmalloc_page_mapped_in_current(page_virt) {
        mm::vmalloc::iounmap(io);
        return fail(
            "vmalloc_heal",
            "ensure_vmalloc_page_mapped_in_current returned false",
        );
    }

    let after_phys = pt_mgr
        .translate_addr(page_virt)
        .map(|p| p & !(mm::PAGE_SIZE - 1));
    if after_phys != before_phys {
        mm::vmalloc::iounmap(io);
        return fail("vmalloc_heal", "healed mapping physical page mismatch");
    }

    mm::vmalloc::iounmap(io);
    pass("vmalloc_heal");
}
