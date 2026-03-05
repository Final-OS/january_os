use super::{fail, mm_step, pass};
use crate::mm;

const TEST_USER_VA_BASE: u64 = mm::USER_MMAP_BASE;
const RECLAIM_STRESS_ITERS: usize = 128;

fn run_reclaim_once(iter: usize) -> Result<(), &'static str> {
    let init_mm = mm::init_mm_ptr();
    let cloned_mm = mm::mm_clone(init_mm);
    if cloned_mm.is_null() {
        return Err("mm_clone failed");
    }

    let direct_map = mm::direct_map_offset();
    let cloned_pgd = unsafe { (*cloned_mm).pgd };
    let pt_mgr = unsafe { mm::PageTableManager::new(cloned_pgd, direct_map) };

    let mut va = (TEST_USER_VA_BASE + (iter as u64) * 0x20_0000) & !(mm::PAGE_SIZE - 1);
    let mut root_idx = mm::level_index(va, mm::page_levels());
    let mut root_was_present = pt_mgr.root_table().entry(root_idx).is_present();

    if root_was_present {
        // Prefer a clean user root slot so the reclaim assertion can verify
        // "created-by-map" root entries are cleared after unmap.
        let mut found = false;
        let mut probe = mm::USER_MMAP_BASE & !(mm::PAGE_SIZE - 1);
        for _ in 0..1024 {
            if probe >= mm::USER_SPACE_END.saturating_sub(mm::PAGE_SIZE) {
                break;
            }
            let idx = mm::level_index(probe, mm::page_levels());
            if !pt_mgr.root_table().entry(idx).is_present() && pt_mgr.translate_addr(probe).is_none() {
                va = probe;
                root_idx = idx;
                root_was_present = false;
                found = true;
                break;
            }
            probe = probe.saturating_add(0x20_0000);
        }

        if !found && crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "[test/mm][pt_reclaim][diag] no empty user root slot found, fallback va={:#x} root_idx={} root_present=true",
                va,
                root_idx
            );
        }
    }

    let data_page = match mm::alloc_page(mm::GFP_USER) {
        Some(p) => p,
        None => {
            unsafe { mm::mm_release(cloned_mm) };
            return Err("alloc_page(GFP_USER) failed");
        }
    };
    let data_phys = mm::page_to_pfn(data_page) * mm::PAGE_SIZE;

    let user_flags = mm::PTE_PRESENT | mm::PTE_WRITABLE | mm::PTE_USER;
    if unsafe { !pt_mgr.map_page(va, data_phys, user_flags) } {
        unsafe {
            mm::free_page(data_page);
            mm::mm_release(cloned_mm);
        }
        return Err("map_page failed");
    }

    let mapped_phys = pt_mgr.translate_addr(va).map(|p| p & !(mm::PAGE_SIZE - 1));
    if mapped_phys != Some(data_phys) {
        unsafe { mm::mm_release(cloned_mm) };
        return Err("mapped phys mismatch");
    }

    if !pt_mgr.root_table().entry(root_idx).is_present() {
        unsafe { mm::mm_release(cloned_mm) };
        return Err("root entry missing after map");
    }

    if unsafe { !pt_mgr.unmap_page(va) } {
        unsafe { mm::mm_release(cloned_mm) };
        return Err("unmap_page failed");
    }

    if pt_mgr.translate_addr(va).is_some() {
        unsafe { mm::mm_release(cloned_mm) };
        return Err("translation still present after unmap");
    }

    let root_present_after = pt_mgr.root_table().entry(root_idx).is_present();
    if !root_was_present && root_present_after {
        unsafe { mm::mm_release(cloned_mm) };
        return Err("reclaim did not clear root entry");
    }
    if root_was_present && !root_present_after {
        unsafe { mm::mm_release(cloned_mm) };
        return Err("reclaim unexpectedly cleared pre-existing root entry");
    }

    unsafe {
        mm::free_page(data_page);
        mm::mm_release(cloned_mm);
    }
    Ok(())
}

pub(super) fn run() {
    mm_step("pt_reclaim: case=clone_map_unmap_release_stress");
    let base_stats = mm::pt_reclaim_stats();

    for iter in 0..RECLAIM_STRESS_ITERS {
        if let Err(msg) = run_reclaim_once(iter) {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "[test/mm][pt_reclaim] failed at iter={} of {}: {}",
                    iter,
                    RECLAIM_STRESS_ITERS,
                    msg
                );
            }
            return fail("pt_reclaim", msg);
        }
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "[test/mm][pt_reclaim] completed {} iterations",
            RECLAIM_STRESS_ITERS
        );
    }

    let after_stats = mm::pt_reclaim_stats();
    if after_stats.stop_non_pgtable != base_stats.stop_non_pgtable {
        return fail(
            "pt_reclaim",
            "pt reclaim non-pgtable stop counter increased",
        );
    }
    if after_stats.stop_shared != base_stats.stop_shared {
        return fail("pt_reclaim", "pt reclaim shared-stop counter increased");
    }
    if after_stats.owner_mismatch != base_stats.owner_mismatch {
        return fail("pt_reclaim", "pt reclaim owner-mismatch counter increased");
    }
    if after_stats.owner_healed != base_stats.owner_healed {
        return fail("pt_reclaim", "pt reclaim owner-healed counter increased");
    }

    pass("pt_reclaim");
}
