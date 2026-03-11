use super::*;

#[derive(Clone)]
pub(crate) struct VmaSegment {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) info: mm::VmaInfo,
}

#[derive(Clone, Copy)]
pub(crate) struct BackingAdjust {
    backing_id: u64,
    retain_count: usize,
    release_count: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct PteUpdate {
    pub(crate) start: u64,
    pub(crate) end: u64,
    old_flags: u64,
    new_flags: u64,
}

#[inline]
pub(crate) fn page_align_up_usize(value: usize) -> Result<usize, i32> {
    let page = mm::PAGE_SIZE as usize;
    value
        .checked_add(page.saturating_sub(1))
        .map(|v| v & !(page - 1))
        .ok_or(E2BIG)
}

#[inline]
pub(crate) fn mprotect_flags_from_prot(old: mm::VmFlags, prot: u32) -> mm::VmFlags {
    let mut flags = old;
    flags.clear(mm::VmFlags::READ | mm::VmFlags::WRITE | mm::VmFlags::EXEC);

    if (prot & mm::prot_flags::PROT_READ) != 0 {
        flags.set(mm::VmFlags::READ);
    }
    if (prot & mm::prot_flags::PROT_WRITE) != 0 {
        flags.set(mm::VmFlags::WRITE);
    }
    if (prot & mm::prot_flags::PROT_EXEC) != 0 {
        flags.set(mm::VmFlags::EXEC);
    }
    if (prot & mm::prot_flags::PROT_WRITE) != 0 || old.contains(mm::VmFlags::MAYWRITE) {
        flags.set(mm::VmFlags::MAYWRITE);
    }

    flags
}

pub(crate) fn apply_pte_flags_range(
    pgd: u64,
    start: u64,
    end: u64,
    pte_flags: u64,
) -> Result<(), i32> {
    let pt_mgr = unsafe { mm::PageTableManager::new(pgd, mm::direct_map_offset()) };
    let mut cursor = start;
    while cursor < end {
        if let Some(phys) = pt_mgr.translate_addr(cursor) {
            let phys_page = phys & !(mm::PAGE_SIZE - 1);
            if unsafe { !pt_mgr.map_page(cursor, phys_page, pte_flags) } {
                return Err(ENOMEM);
            }
        }
        cursor = cursor.saturating_add(mm::PAGE_SIZE);
    }
    Ok(())
}

#[inline]
pub(crate) fn vma_backing_id(info: &mm::VmaInfo) -> Option<u64> {
    if info.file.is_null() {
        None
    } else {
        Some(info.file as usize as u64)
    }
}

#[inline]
pub(crate) fn clone_vma_slice(base: &VmaSegment, start: u64, end: u64) -> VmaSegment {
    let mut info = base.info.clone();
    let pgoff_delta = (start.saturating_sub(base.start)) / mm::PAGE_SIZE;
    info.pgoff = info.pgoff.saturating_add(pgoff_delta);
    VmaSegment { start, end, info }
}

#[inline]
pub(crate) fn backing_adjust_for_segments(
    base: &VmaSegment,
    kept_segments: usize,
) -> Option<BackingAdjust> {
    let backing_id = vma_backing_id(&base.info)?;
    Some(BackingAdjust {
        backing_id,
        retain_count: kept_segments.saturating_sub(1),
        release_count: usize::from(kept_segments == 0),
    })
}

pub(crate) fn apply_backing_retains(adjustments: &[BackingAdjust]) -> Result<(), i32> {
    let mut retained: Vec<u64> = Vec::new();

    for adjust in adjustments {
        for _ in 0..adjust.retain_count {
            if let Err(errno) = fs::backing::retain_mmap_backing(adjust.backing_id) {
                for backing_id in retained.into_iter().rev() {
                    fs::backing::release_mmap_backing(backing_id);
                }
                return Err(errno);
            }
            retained.push(adjust.backing_id);
        }
    }

    Ok(())
}

pub(crate) fn rollback_backing_retains(adjustments: &[BackingAdjust]) {
    for adjust in adjustments.iter().rev() {
        for _ in 0..adjust.retain_count {
            fs::backing::release_mmap_backing(adjust.backing_id);
        }
    }
}

pub(crate) fn apply_backing_releases(adjustments: &[BackingAdjust]) {
    for adjust in adjustments {
        for _ in 0..adjust.release_count {
            fs::backing::release_mmap_backing(adjust.backing_id);
        }
    }
}

#[inline]
pub(crate) fn mm_insert_vma_locked(mm_state: &mut mm::Mm, segment: &VmaSegment) -> bool {
    let nr_pages = (segment.end - segment.start) / mm::PAGE_SIZE;
    let flags = segment.info.flags;

    if mm_state
        .vma_tree
        .insert(
            segment.start as usize,
            segment.end as usize,
            segment.info.clone(),
        )
        .is_err()
    {
        return false;
    }

    mm_state.vma_count = mm_state.vma_count.saturating_add(1);
    mm_state.total_vm = mm_state.total_vm.saturating_add(nr_pages);
    if flags.contains(mm::VmFlags::EXEC) {
        mm_state.exec_vm = mm_state.exec_vm.saturating_add(nr_pages);
    }
    if flags.contains(mm::VmFlags::GROWSDOWN) {
        mm_state.stack_vm = mm_state.stack_vm.saturating_add(nr_pages);
    }
    true
}

#[inline]
pub(crate) fn mm_remove_vma_locked(mm_state: &mut mm::Mm, start: u64) -> Option<VmaSegment> {
    let (end, info) = mm_state.vma_tree.remove(start as usize)?;
    let end = end as u64;
    let nr_pages = (end - start) / mm::PAGE_SIZE;

    mm_state.vma_count = mm_state.vma_count.saturating_sub(1);
    mm_state.total_vm = mm_state.total_vm.saturating_sub(nr_pages);
    if info.flags.contains(mm::VmFlags::EXEC) {
        mm_state.exec_vm = mm_state.exec_vm.saturating_sub(nr_pages);
    }
    if info.flags.contains(mm::VmFlags::GROWSDOWN) {
        mm_state.stack_vm = mm_state.stack_vm.saturating_sub(nr_pages);
    }

    Some(VmaSegment { start, end, info })
}

pub(crate) fn rollback_vma_transaction(
    mm_state: &mut mm::Mm,
    removed: &[VmaSegment],
    inserted: &[VmaSegment],
) -> bool {
    let mut ok = true;

    for segment in inserted.iter().rev() {
        if mm_remove_vma_locked(mm_state, segment.start).is_none() {
            ok = false;
        }
    }

    for segment in removed {
        if !mm_insert_vma_locked(mm_state, segment) {
            ok = false;
        }
    }

    ok
}

pub(crate) fn plan_mprotect_updates(
    mm_state: &mm::Mm,
    start: u64,
    end: u64,
    prot: u32,
) -> Result<
    (
        Vec<VmaSegment>,
        Vec<VmaSegment>,
        Vec<BackingAdjust>,
        Vec<PteUpdate>,
    ),
    i32,
> {
    let mut cursor = start;
    let mut removed: Vec<VmaSegment> = Vec::new();
    let mut inserted: Vec<VmaSegment> = Vec::new();
    let mut backing_adjusts: Vec<BackingAdjust> = Vec::new();
    let mut pte_updates: Vec<PteUpdate> = Vec::new();

    while cursor < end {
        let Some((vma_start, vma_end, info)) = mm_state.vma_tree.find(cursor as usize) else {
            return Err(ENOMEM);
        };
        let vma_start = vma_start as u64;
        let vma_end = vma_end as u64;
        if vma_start > cursor {
            return Err(ENOMEM);
        }

        let original = VmaSegment {
            start: vma_start,
            end: vma_end,
            info: info.clone(),
        };
        let seg_start = cursor;
        let seg_end = cmp::min(vma_end, end);
        let mut kept_segments = 0usize;

        removed.push(original.clone());
        if original.start < seg_start {
            inserted.push(clone_vma_slice(&original, original.start, seg_start));
            kept_segments += 1;
        }

        let mut protected = clone_vma_slice(&original, seg_start, seg_end);
        protected.info.flags = mprotect_flags_from_prot(original.info.flags, prot);
        inserted.push(protected.clone());
        kept_segments += 1;

        pte_updates.push(PteUpdate {
            start: seg_start,
            end: seg_end,
            old_flags: original.info.flags.to_user_pte_flags(),
            new_flags: protected.info.flags.to_user_pte_flags(),
        });

        if seg_end < original.end {
            inserted.push(clone_vma_slice(&original, seg_end, original.end));
            kept_segments += 1;
        }

        if let Some(adjust) = backing_adjust_for_segments(&original, kept_segments) {
            backing_adjusts.push(adjust);
        }

        cursor = seg_end;
    }

    Ok((removed, inserted, backing_adjusts, pte_updates))
}

pub(crate) fn plan_unmap_changes(
    mm_state: &mm::Mm,
    addr: u64,
    end: u64,
) -> Result<
    (
        Vec<VmaSegment>,
        Vec<VmaSegment>,
        Vec<BackingAdjust>,
        Vec<(u64, u64)>,
    ),
    i32,
> {
    let mut removed: Vec<VmaSegment> = Vec::new();
    let mut inserted: Vec<VmaSegment> = Vec::new();
    let mut backing_adjusts: Vec<BackingAdjust> = Vec::new();
    let mut unmap_ranges: Vec<(u64, u64)> = Vec::new();

    let mut cursor = addr;
    while let Some((vma_start, vma_end, info)) = mm_state
        .vma_tree
        .iter_intersecting(cursor as usize, end as usize)
        .next()
    {
        let original = VmaSegment {
            start: vma_start as u64,
            end: vma_end as u64,
            info: info.clone(),
        };
        let cut_start = cmp::max(original.start, addr);
        let cut_end = cmp::min(original.end, end);
        let mut kept_segments = 0usize;

        removed.push(original.clone());
        if original.start < cut_start {
            inserted.push(clone_vma_slice(&original, original.start, cut_start));
            kept_segments += 1;
        }
        if cut_end < original.end {
            inserted.push(clone_vma_slice(&original, cut_end, original.end));
            kept_segments += 1;
        }

        if let Some(adjust) = backing_adjust_for_segments(&original, kept_segments) {
            backing_adjusts.push(adjust);
        }
        unmap_ranges.push((cut_start, cut_end));
        cursor = cut_end;
        if cursor >= end {
            break;
        }
    }

    Ok((removed, inserted, backing_adjusts, unmap_ranges))
}

pub(crate) fn mprotect_range_for_mm(
    mm_state: &mut mm::Mm,
    start: u64,
    end: u64,
    prot: u32,
) -> Result<(), i32> {
    let (removed, inserted, backing_adjusts, pte_updates) = {
        let _guard = mm_state.lock.lock();
        plan_mprotect_updates(mm_state, start, end, prot)?
    };

    apply_backing_retains(&backing_adjusts)?;

    {
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
    }

    let mut applied_updates: Vec<PteUpdate> = Vec::new();
    for update in &pte_updates {
        if let Err(errno) =
            apply_pte_flags_range(mm_state.pgd, update.start, update.end, update.new_flags)
        {
            for applied in applied_updates.iter().rev() {
                let _ = apply_pte_flags_range(
                    mm_state.pgd,
                    applied.start,
                    applied.end,
                    applied.old_flags,
                );
            }
            let mm_ptr = mm_state as *mut mm::Mm;
            let _guard = unsafe { (*core::ptr::addr_of!((*mm_ptr).lock)).lock() };
            let _ = unsafe { rollback_vma_transaction(&mut *mm_ptr, &removed, &inserted) };
            rollback_backing_retains(&backing_adjusts);
            return Err(errno);
        }
        applied_updates.push(*update);
    }

    Ok(())
}
