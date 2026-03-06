use alloc::vec::Vec;
use core::cmp;

use crate::fs;
use crate::mm;
use crate::syscall::{
    err, ok, SyscallArgs, SyscallRet, E2BIG, EBADF, EBUSY, EINVAL, ENOMEM, ESRCH,
};
use crate::task;

const MMAP_PROT_ALLOWED: u32 =
    mm::prot_flags::PROT_READ | mm::prot_flags::PROT_WRITE | mm::prot_flags::PROT_EXEC;
const MMAP_FLAGS_ALLOWED: u32 = mm::mmap_flags::MAP_SHARED
    | mm::mmap_flags::MAP_PRIVATE
    | mm::mmap_flags::MAP_FIXED
    | mm::mmap_flags::MAP_ANONYMOUS
    | mm::mmap_flags::MAP_GROWSDOWN
    | mm::mmap_flags::MAP_LOCKED
    | mm::mmap_flags::MAP_HUGETLB;
const MMAP_FLAGS_UNSUPPORTED: u32 = mm::mmap_flags::MAP_LOCKED | mm::mmap_flags::MAP_HUGETLB;

#[derive(Clone)]
struct VmaSegment {
    start: u64,
    end: u64,
    info: mm::VmaInfo,
}

#[derive(Clone, Copy)]
struct BackingAdjust {
    backing_id: u64,
    retain_count: usize,
    release_count: usize,
}

#[derive(Clone, Copy)]
struct PteUpdate {
    start: u64,
    end: u64,
    old_flags: u64,
    new_flags: u64,
}

#[inline]
fn page_align_up_usize(value: usize) -> Result<usize, i32> {
    let page = mm::PAGE_SIZE as usize;
    value
        .checked_add(page.saturating_sub(1))
        .map(|v| v & !(page - 1))
        .ok_or(E2BIG)
}

#[inline]
fn mprotect_flags_from_prot(old: mm::VmFlags, prot: u32) -> mm::VmFlags {
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

fn apply_pte_flags_range(pgd: u64, start: u64, end: u64, pte_flags: u64) -> Result<(), i32> {
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
fn vma_backing_id(info: &mm::VmaInfo) -> Option<u64> {
    if info.file.is_null() {
        None
    } else {
        Some(info.file as usize as u64)
    }
}

#[inline]
fn clone_vma_slice(base: &VmaSegment, start: u64, end: u64) -> VmaSegment {
    let mut info = base.info.clone();
    let pgoff_delta = (start.saturating_sub(base.start)) / mm::PAGE_SIZE;
    info.pgoff = info.pgoff.saturating_add(pgoff_delta);
    VmaSegment { start, end, info }
}

#[inline]
fn backing_adjust_for_segments(base: &VmaSegment, kept_segments: usize) -> Option<BackingAdjust> {
    let backing_id = vma_backing_id(&base.info)?;
    Some(BackingAdjust {
        backing_id,
        retain_count: kept_segments.saturating_sub(1),
        release_count: usize::from(kept_segments == 0),
    })
}

fn apply_backing_retains(adjustments: &[BackingAdjust]) -> Result<(), i32> {
    let mut retained: Vec<u64> = Vec::new();

    for adjust in adjustments {
        for _ in 0..adjust.retain_count {
            if let Err(errno) = fs::mmap_retain_backing(adjust.backing_id) {
                for backing_id in retained.into_iter().rev() {
                    fs::mmap_release_backing(backing_id);
                }
                return Err(errno);
            }
            retained.push(adjust.backing_id);
        }
    }

    Ok(())
}

fn rollback_backing_retains(adjustments: &[BackingAdjust]) {
    for adjust in adjustments.iter().rev() {
        for _ in 0..adjust.retain_count {
            fs::mmap_release_backing(adjust.backing_id);
        }
    }
}

fn apply_backing_releases(adjustments: &[BackingAdjust]) {
    for adjust in adjustments {
        for _ in 0..adjust.release_count {
            fs::mmap_release_backing(adjust.backing_id);
        }
    }
}

#[inline]
fn mm_insert_vma_locked(mm_state: &mut mm::Mm, segment: &VmaSegment) -> bool {
    let nr_pages = (segment.end - segment.start) / mm::PAGE_SIZE;
    let flags = segment.info.flags;

    if mm_state
        .vma_tree
        .insert(segment.start as usize, segment.end as usize, segment.info.clone())
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
fn mm_remove_vma_locked(mm_state: &mut mm::Mm, start: u64) -> Option<VmaSegment> {
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

fn rollback_vma_transaction(
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

fn plan_mprotect_updates(
    mm_state: &mm::Mm,
    start: u64,
    end: u64,
    prot: u32,
) -> Result<(Vec<VmaSegment>, Vec<VmaSegment>, Vec<BackingAdjust>, Vec<PteUpdate>), i32> {
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

fn plan_unmap_changes(
    mm_state: &mm::Mm,
    addr: u64,
    end: u64,
) -> Result<(Vec<VmaSegment>, Vec<VmaSegment>, Vec<BackingAdjust>, Vec<(u64, u64)>), i32> {
    let mut removed: Vec<VmaSegment> = Vec::new();
    let mut inserted: Vec<VmaSegment> = Vec::new();
    let mut backing_adjusts: Vec<BackingAdjust> = Vec::new();
    let mut unmap_ranges: Vec<(u64, u64)> = Vec::new();

    let mut cursor = addr;
    while let Some((vma_start, vma_end, info)) = mm_state.vma_tree.iter_intersecting(cursor as usize, end as usize).next() {
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

fn mprotect_range_for_mm(
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
        if let Err(errno) = apply_pte_flags_range(mm_state.pgd, update.start, update.end, update.new_flags)
        {
            for applied in applied_updates.iter().rev() {
                let _ = apply_pte_flags_range(mm_state.pgd, applied.start, applied.end, applied.old_flags);
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

#[inline]
fn current_pid_raw() -> Result<usize, i32> {
    task::current_pid().map(|pid| pid.0).ok_or(ESRCH)
}

#[inline]
fn parse_fd(raw: usize) -> Result<i32, i32> {
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
fn validate_user_mmap_range(start: u64, len: u64) -> Result<(), i32> {
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

fn mmap_select_addr(
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

fn range_unmapped_in_page_table(pt_mgr: &mm::PageTableManager, start: u64, end: u64) -> bool {
    let mut cursor = start;
    while cursor < end {
        if pt_mgr.translate_addr(cursor).is_some() {
            return false;
        }
        cursor = cursor.saturating_add(mm::PAGE_SIZE);
    }
    true
}

unsafe fn unmap_and_release_pages(start: u64, end: u64, pgd: u64) {
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

fn collect_unmap_ranges_for_mm(
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

pub(crate) fn sys_mmap(args: &SyscallArgs) -> SyscallRet {
    let req_addr = args.arg0;
    let length = args.arg1;
    let prot = args.arg2 as u32;
    let flags = args.arg3 as u32;
    let fd_raw = args.arg4;
    let offset = args.arg5 as u64;

    if length == 0 {
        return err(EINVAL);
    }
    if (prot & !MMAP_PROT_ALLOWED) != 0 {
        return err(EINVAL);
    }
    if (flags & !MMAP_FLAGS_ALLOWED) != 0 {
        return err(EINVAL);
    }
    if (flags & MMAP_FLAGS_UNSUPPORTED) != 0 {
        return err(EINVAL);
    }

    let shared = (flags & mm::mmap_flags::MAP_SHARED) != 0;
    let private = (flags & mm::mmap_flags::MAP_PRIVATE) != 0;
    if shared == private {
        return err(EINVAL);
    }

    if (offset & (mm::PAGE_SIZE - 1)) != 0 {
        return err(EINVAL);
    }

    let length_aligned = match page_align_up_usize(length) {
        Ok(value) => value as u64,
        Err(errno) => return err(errno),
    };
    if length_aligned == 0 {
        return err(E2BIG);
    }

    let vm_flags = mm::mmap_flags_to_vm_flags(prot, flags);
    let map_fixed = (flags & mm::mmap_flags::MAP_FIXED) != 0;
    let start = match mmap_select_addr(req_addr, length_aligned, flags, vm_flags) {
        Ok(addr) => addr,
        Err(errno) => return err(errno),
    };
    let end = match start.checked_add(length_aligned) {
        Some(value) => value,
        None => return err(ENOMEM),
    };

    let mm_ptr = task::current_mm_ptr();
    let mm_pgd = unsafe { (*mm_ptr).pgd };
    if map_fixed {
        let unmap_ranges = {
            let mm_state = unsafe { &mut *mm_ptr };
            match collect_unmap_ranges_for_mm(mm_state, start, end) {
                Ok(ranges) => ranges,
                Err(errno) => return err(errno),
            }
        };

        for (range_start, range_end) in unmap_ranges.into_iter() {
            unsafe {
                unmap_and_release_pages(range_start, range_end, mm_pgd);
            }
        }
    }

    let mut info = mm::VmaInfo::new(vm_flags);
    info.pgoff = offset / mm::PAGE_SIZE;
    let mut file_backing_id: Option<u64> = None;
    if (flags & mm::mmap_flags::MAP_ANONYMOUS) == 0 {
        let fd = match parse_fd(fd_raw) {
            Ok(fd) => fd,
            Err(errno) => return err(errno),
        };
        let pid = match current_pid_raw() {
            Ok(pid) => pid,
            Err(errno) => return err(errno),
        };
        let backing_id = match fs::mmap_create_backing_for_pid(pid, fd) {
            Ok(id) => id,
            Err(errno) => return err(errno),
        };
        info.file = backing_id as usize as *mut ();
        info.private_data = core::ptr::null_mut();
        file_backing_id = Some(backing_id);
    }

    let mm_state = unsafe { &mut *mm_ptr };
    if mm_state.find_vma_intersection(start, end).is_some() {
        if let Some(backing_id) = file_backing_id {
            fs::mmap_release_backing(backing_id);
        }
        return err(EBUSY);
    }
    if !mm_state.insert_vma(start, end, info) {
        if let Some(backing_id) = file_backing_id {
            fs::mmap_release_backing(backing_id);
        }
        return err(EBUSY);
    }

    ok(start as usize)
}

pub(crate) fn sys_munmap(args: &SyscallArgs) -> SyscallRet {
    let addr = args.arg0 as u64;
    let length = args.arg1;

    if length == 0 {
        return err(EINVAL);
    }
    if (addr & (mm::PAGE_SIZE - 1)) != 0 {
        return err(EINVAL);
    }

    let length_aligned = match page_align_up_usize(length) {
        Ok(value) => value as u64,
        Err(errno) => return err(errno),
    };
    let end = match addr.checked_add(length_aligned) {
        Some(v) => v,
        None => return err(ENOMEM),
    };
    if addr < mm::USER_SPACE_START || end > mm::USER_SPACE_END || end <= addr {
        return err(EINVAL);
    }

    let mm_ptr = task::current_mm_ptr();
    let mm_pgd = unsafe { (*mm_ptr).pgd };
    let unmap_ranges = {
        let mm_state = unsafe { &mut *mm_ptr };
        match collect_unmap_ranges_for_mm(mm_state, addr, end) {
            Ok(ranges) => ranges,
            Err(errno) => return err(errno),
        }
    };

    for (range_start, range_end) in unmap_ranges.into_iter() {
        unsafe {
            unmap_and_release_pages(range_start, range_end, mm_pgd);
        }
    }

    ok(0)
}

pub(crate) fn sys_brk(args: &SyscallArgs) -> SyscallRet {
    let req = args.arg0 as u64;
    let mm_ptr = task::current_mm_ptr();
    if mm_ptr.is_null() {
        return ok(0);
    }
    let mm_state = unsafe { &mut *mm_ptr };

    if mm_state.start_brk == 0 {
        let hint = mm::USER_SPACE_START.saturating_add(0x0100_0000);
        let base = mm_state
            .find_free_area(hint, mm::PAGE_SIZE, mm::VmFlags::empty())
            .unwrap_or(hint);
        mm_state.start_brk = base;
        mm_state.brk = base;
    }

    if req == 0 {
        return ok(mm_state.brk as usize);
    }
    if req < mm_state.start_brk || req >= mm::USER_SPACE_END {
        return ok(mm_state.brk as usize);
    }

    match mm_state.do_brk(req) {
        Ok(new_brk) => ok(new_brk as usize),
        Err(_) => ok(mm_state.brk as usize),
    }
}

pub(crate) fn sys_mprotect(args: &SyscallArgs) -> SyscallRet {
    let addr = args.arg0 as u64;
    let len = args.arg1;
    let prot = args.arg2 as u32;

    if len == 0 {
        return err(EINVAL);
    }
    if (addr & (mm::PAGE_SIZE - 1)) != 0 {
        return err(EINVAL);
    }
    if (prot & !MMAP_PROT_ALLOWED) != 0 {
        return err(EINVAL);
    }

    let len_aligned = match page_align_up_usize(len) {
        Ok(v) => v as u64,
        Err(errno) => return err(errno),
    };
    let end = match addr.checked_add(len_aligned) {
        Some(v) => v,
        None => return err(ENOMEM),
    };
    if addr < mm::USER_SPACE_START || end > mm::USER_SPACE_END || end <= addr {
        return err(ENOMEM);
    }

    let mm_ptr = task::current_mm_ptr();
    if mm_ptr.is_null() {
        return err(ENOMEM);
    }
    let mm_state = unsafe { &mut *mm_ptr };
    match mprotect_range_for_mm(mm_state, addr, end, prot) {
        Ok(()) => ok(0),
        Err(errno) => err(errno),
    }
}
