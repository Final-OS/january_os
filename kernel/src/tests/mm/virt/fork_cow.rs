use super::{fail, mm_step, pass};
use crate::mm;

pub(super) fn run() {
    mm_step("fork_cow: case=mm_clone_shares_private_pages_until_write_fault");

    let init_mm = mm::init_mm_ptr();
    let parent_mm = mm::mm_clone(init_mm);
    if parent_mm.is_null() {
        return fail("fork_cow", "parent mm_clone(init_mm) failed");
    }

    let result = (|| -> Result<(), &'static str> {
        let test_va = unsafe { &*parent_mm }
            .find_free_area(
                mm::USER_SPACE_START.saturating_add(0x80_0000),
                mm::PAGE_SIZE,
                mm::VmFlags::empty(),
            )
            .unwrap_or(mm::USER_SPACE_START.saturating_add(0x80_0000));

        let page = mm::alloc_page(mm::GFP_USER).ok_or("alloc_page failed")?;
        page.set_flag(mm::PageFlags::UPTODATE);
        page.set_flag(mm::PageFlags::ANON);
        let phys = mm::page_to_pfn(page) * mm::PAGE_SIZE;
        let virt = mm::phys_to_virt(phys);
        unsafe {
            core::ptr::write(virt as *mut u64, 0x1122_3344_5566_7788);
        }

        let mut flags = mm::VmFlags::empty();
        flags.set(mm::VmFlags::READ);
        flags.set(mm::VmFlags::WRITE);
        flags.set(mm::VmFlags::MAYWRITE);
        flags.set(mm::VmFlags::ANONYMOUS);

        let parent_pt = unsafe { mm::PageTableManager::new((*parent_mm).pgd, mm::direct_map_offset()) };
        if unsafe { !parent_pt.map_page(test_va, phys, flags.to_user_pte_flags()) } {
            return Err("parent map_page failed");
        }
        page.inc_mapcount();
        if unsafe { &mut *parent_mm }.insert_vma(test_va, test_va + mm::PAGE_SIZE, mm::VmaInfo::new(flags)) == false {
            return Err("parent insert_vma failed");
        }

        let child_mm = mm::mm_clone(parent_mm);
        if child_mm.is_null() {
            return Err("child mm_clone failed");
        }

        let result = (|| -> Result<(), &'static str> {
            let child_pt = unsafe { mm::PageTableManager::new((*child_mm).pgd, mm::direct_map_offset()) };
            let parent_entry = parent_pt.translate(test_va).ok_or("parent translate missing")?.0;
            let child_entry = child_pt.translate(test_va).ok_or("child translate missing")?.0;

            if parent_entry.phys_addr() != child_entry.phys_addr() {
                return Err("fork clone should initially share same physical page");
            }
            if parent_entry.is_writable() || child_entry.is_writable() {
                return Err("fork clone should downgrade private mappings to read-only");
            }

            let mut fault = mm::FaultContext::new(test_va, 0x7, child_mm, mm::direct_map_offset());
            let cow_result = mm::handle_page_fault(&mut fault);
            if cow_result != mm::FaultResult::Retry {
                return Err("child write fault did not resolve as COW retry");
            }

            let parent_after = parent_pt.translate(test_va).ok_or("parent translate after cow missing")?.0;
            let child_after = child_pt.translate(test_va).ok_or("child translate after cow missing")?.0;
            if parent_after.phys_addr() == child_after.phys_addr() {
                return Err("child COW fault did not allocate a new physical page");
            }
            if !child_after.is_writable() {
                return Err("child COW mapping should become writable");
            }
            if parent_after.is_writable() {
                return Err("parent mapping should remain read-only after child COW");
            }

            let child_value = unsafe { core::ptr::read(mm::phys_to_virt(child_after.phys_addr()) as *const u64) };
            let parent_value = unsafe { core::ptr::read(mm::phys_to_virt(parent_after.phys_addr()) as *const u64) };
            if child_value != 0x1122_3344_5566_7788 || parent_value != 0x1122_3344_5566_7788 {
                return Err("COW copy did not preserve original page contents");
            }

            Ok(())
        })();

        unsafe { mm::mm_release(child_mm) };
        result
    })();

    unsafe { mm::mm_release(parent_mm) };

    match result {
        Ok(()) => pass("fork_cow"),
        Err(msg) => fail("fork_cow", msg),
    }
}
