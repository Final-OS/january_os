use super::*;

pub(crate) fn current_pid_raw() -> Result<usize, i32> {
    task::current_pid().map(|pid| pid.0).ok_or(ESRCH)
}

#[inline]
pub(crate) fn parse_fd(raw: usize) -> Result<i32, i32> {
    if raw > i32::MAX as usize {
        return Err(EBADF);
    }
    let fd = raw as i32;
    if fd < 0 {
        return Err(EBADF);
    }
    Ok(fd)
}

#[inline]
pub(crate) fn validate_user_mmap_range(start: u64, len: u64) -> Result<(), i32> {
    if len == 0 {
        return Err(EINVAL);
    }
    let end = start.checked_add(len).ok_or(ENOMEM)?;
    if start < mm::USER_SPACE_START {
        return Err(EINVAL);
    }
    if end <= start {
        return Err(ENOMEM);
    }
    if end > mm::USER_SPACE_END {
        return Err(ENOMEM);
    }
    if !mm::is_user_addr(start) || !mm::is_user_addr(end - 1) {
        return Err(ENOMEM);
    }
    Ok(())
}

pub(crate) fn mmap_select_addr(
    req_addr: usize,
    len_aligned: u64,
    flags: u32,
    vm_flags: mm::VmFlags,
) -> Result<u64, i32> {
    let map_fixed = (flags & mm::mmap_flags::MAP_FIXED) != 0;
    let mm_ptr = task::current_mm_ptr();
    let mm_state = unsafe { &mut *mm_ptr };
    let pt_mgr = unsafe { mm::PageTableManager::new(mm_state.pgd, mm::direct_map_offset()) };
    if map_fixed {
        let start = req_addr as u64;
        if (start & (mm::PAGE_SIZE - 1)) != 0 {
            return Err(EINVAL);
        }
        validate_user_mmap_range(start, len_aligned)?;
        return Ok(start);
    }

    let mut hint = if req_addr == 0 {
        mm_state.mmap_base.max(mm::USER_SPACE_START)
    } else {
        mm::page_align_up(req_addr as u64).max(mm::USER_SPACE_START)
    };

    // 在 VMA 空洞中进一步过滤“页表已有映射”的区间，避免覆盖早期恒等映射等遗留映射。
    for _ in 0..1024 {
        let Some(start) = mm_state.find_free_area(hint, len_aligned, vm_flags) else {
            return Err(ENOMEM);
        };
        validate_user_mmap_range(start, len_aligned)?;
        let end = start.checked_add(len_aligned).ok_or(ENOMEM)?;
        if range_unmapped_in_page_table(&pt_mgr, start, end) {
            return Ok(start);
        }

        hint = end;
        if hint >= mm::USER_SPACE_END {
            return Err(ENOMEM);
        }
    }

    if hint != mm::USER_SPACE_START {
        hint = mm::USER_SPACE_START;
        for _ in 0..1024 {
            let Some(start) = mm_state.find_free_area(hint, len_aligned, vm_flags) else {
                return Err(ENOMEM);
            };
            validate_user_mmap_range(start, len_aligned)?;
            let end = start.checked_add(len_aligned).ok_or(ENOMEM)?;
            if range_unmapped_in_page_table(&pt_mgr, start, end) {
                return Ok(start);
            }

            hint = end;
            if hint >= mm::USER_SPACE_END {
                return Err(ENOMEM);
            }
        }
    }

    Err(ENOMEM)
}

pub(crate) fn range_unmapped_in_page_table(pt_mgr: &mm::PageTableManager, start: u64, end: u64) -> bool {
    let mut cursor = start;
    while cursor < end {
        if pt_mgr.translate_addr(cursor).is_some() {
            return false;
        }
        cursor = cursor.saturating_add(mm::PAGE_SIZE);
    }
    true
}

pub(crate) unsafe fn unmap_and_release_pages(start: u64, end: u64, pgd: u64) {
    let pt_mgr = mm::PageTableManager::new(pgd, mm::direct_map_offset());

    let mut cursor = start;
    while cursor < end {
        if let Some(phys) = pt_mgr.translate_addr(cursor) {
            let _ = pt_mgr.unmap_page(cursor);
            let pfn = phys / mm::PAGE_SIZE;
            if pfn < mm::max_pfn() {
                let page = &mut *mm::pfn_to_page(pfn);
                if page.mapcount() >= 0 {
                    let _ = page.try_dec_mapcount();
                }
                if !page.is_reserved() && page.refcount() > 0 {
                    mm::free_page(page);
                }
            }
        }
        cursor = cursor.saturating_add(mm::PAGE_SIZE);
    }
}

pub(crate) fn collect_unmap_ranges_for_mm(
    mm_state: &mut mm::Mm,
    addr: u64,
    end: u64,
) -> Result<Vec<(u64, u64)>, i32> {
    let (removed, inserted, backing_adjusts, unmap_ranges) = {
        let _guard = mm_state.lock.lock();
        plan_unmap_changes(mm_state, addr, end)?
    };

    apply_backing_retains(&backing_adjusts)?;

    let mm_ptr = mm_state as *mut mm::Mm;
    let _guard = unsafe { (*core::ptr::addr_of!((*mm_ptr).lock)).lock() };
    for segment in &removed {
        if unsafe { mm_remove_vma_locked(&mut *mm_ptr, segment.start) }.is_none() {
            rollback_backing_retains(&backing_adjusts);
            return Err(EBUSY);
        }
    }

    let mut inserted_now: Vec<VmaSegment> = Vec::new();
    for segment in &inserted {
        if !unsafe { mm_insert_vma_locked(&mut *mm_ptr, segment) } {
            let _ = unsafe { rollback_vma_transaction(&mut *mm_ptr, &removed, &inserted_now) };
            rollback_backing_retains(&backing_adjusts);
            return Err(EBUSY);
        }
        inserted_now.push(segment.clone());
    }

    apply_backing_releases(&backing_adjusts);
    Ok(unmap_ranges)
}
