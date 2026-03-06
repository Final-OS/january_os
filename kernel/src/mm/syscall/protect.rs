use super::*;

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
