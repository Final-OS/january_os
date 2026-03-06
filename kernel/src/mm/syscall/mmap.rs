use super::*;

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
        let backing_id = match fs::backing::create_mmap_backing_for_pid(pid, fd) {
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
            fs::backing::release_mmap_backing(backing_id);
        }
        return err(EBUSY);
    }
    if !mm_state.insert_vma(start, end, info) {
        if let Some(backing_id) = file_backing_id {
            fs::backing::release_mmap_backing(backing_id);
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
