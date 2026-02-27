use super::{fail, mm_step, pass};
use crate::kprintln;
use crate::syscall;
use crate::syscall::handlers::{sys_mmap, sys_munmap};

#[inline]
fn ret_is_err(ret: usize) -> bool {
    (ret as isize) < 0
}

#[inline]
fn ret_errno(ret: usize) -> i32 {
    (-(ret as isize)) as i32
}

#[inline]
fn expect_errno(case: &str, ret: usize, expect: i32) -> Result<(), &'static str> {
    if !ret_is_err(ret) {
        kprintln!(
            "[test/mm][mmap][{}] expected errno={} actual=ok ret={:#x}",
            case,
            expect,
            ret
        );
        return Err("expected syscall error but got success");
    }
    let got = ret_errno(ret);
    kprintln!(
        "[test/mm][mmap][{}] expected errno={} actual_errno={}",
        case,
        expect,
        got
    );
    if got != expect {
        return Err("unexpected errno");
    }
    Ok(())
}

#[inline]
fn do_mmap(addr: usize, len: usize, prot: u32, flags: u32, fd: usize, offset: usize) -> usize {
    let args = syscall::SyscallArgs::new(9, addr, len, prot as usize, flags as usize, fd, offset);
    sys_mmap(&args)
}

#[inline]
fn do_munmap(addr: usize, len: usize) -> usize {
    let args = syscall::SyscallArgs::new(11, addr, len, 0, 0, 0, 0);
    sys_munmap(&args)
}

pub(super) fn run() {
    let page = crate::mm::PAGE_SIZE as usize;
    let map_flags = crate::mm::mmap_flags::MAP_PRIVATE | crate::mm::mmap_flags::MAP_ANONYMOUS;
    let prot_rw = crate::mm::prot_flags::PROT_READ | crate::mm::prot_flags::PROT_WRITE;
    let low_hint = crate::mm::USER_SPACE_START;

    mm_step("mmap: case=invalid_zero_length");
    if expect_errno(
        "zero-len",
        do_mmap(0, 0, prot_rw, map_flags, usize::MAX, 0),
        syscall::EINVAL,
    )
    .is_err()
    {
        return fail("mmap", "mmap with zero length must fail with EINVAL");
    }

    mm_step("mmap: case=invalid_map_type_missing_shared_private");
    if expect_errno(
        "missing-map-type",
        do_mmap(
            0,
            page,
            prot_rw,
            crate::mm::mmap_flags::MAP_ANONYMOUS,
            usize::MAX,
            0,
        ),
        syscall::EINVAL,
    )
    .is_err()
    {
        return fail("mmap", "mmap without MAP_SHARED/MAP_PRIVATE must fail");
    }

    mm_step("mmap: case=unsupported_file_backed");
    if expect_errno(
        "file-backed",
        do_mmap(0, page, prot_rw, crate::mm::mmap_flags::MAP_PRIVATE, 3, 0),
        syscall::ENOSYS,
    )
    .is_err()
    {
        return fail("mmap", "file-backed mmap should return ENOSYS in current stage");
    }

    mm_step("mmap: case=invalid_unaligned_fixed_addr");
    if expect_errno(
        "fixed-unaligned",
        do_mmap(
            crate::mm::USER_SPACE_START as usize + 1,
            page,
            prot_rw,
            map_flags | crate::mm::mmap_flags::MAP_FIXED,
            usize::MAX,
            0,
        ),
        syscall::EINVAL,
    )
    .is_err()
    {
        return fail("mmap", "MAP_FIXED with unaligned addr must fail");
    }

    mm_step("mmap: case=anon_private_rw_map_and_access");
    let low_hint_mapped_before = {
        let cr3 = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        let pt_mgr = unsafe { crate::mm::PageTableManager::new(cr3, crate::mm::DIRECT_MAP_OFFSET) };
        pt_mgr.translate_addr(low_hint).is_some()
    };
    kprintln!(
        "[test/mm][mmap][precheck] low_hint={:#x} mapped_before={}",
        low_hint,
        low_hint_mapped_before
    );
    let map_ret = do_mmap(0, page * 2, prot_rw, map_flags, usize::MAX, 0);
    if ret_is_err(map_ret) {
        kprintln!(
            "[test/mm][mmap][map] expected success actual_errno={}",
            ret_errno(map_ret)
        );
        return fail("mmap", "anonymous private mmap failed");
    }
    let map_addr = map_ret as u64;
    kprintln!(
        "[test/mm][mmap][map] addr={:#x} len={} page_size={}",
        map_addr,
        page * 2,
        page
    );
    if (map_addr & (crate::mm::PAGE_SIZE - 1)) != 0 {
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "mmap returned non page-aligned address");
    }
    if map_addr < crate::mm::USER_SPACE_START || !crate::mm::is_user_addr(map_addr) {
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "mmap returned address outside user range");
    }
    if low_hint_mapped_before && map_addr == low_hint {
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "mmap selected pre-mapped low hint address");
    }

    // 触发匿名页缺页分配：写入两个页面并校验可读回。
    unsafe {
        let p0 = map_addr as *mut u64;
        let p1 = (map_addr + crate::mm::PAGE_SIZE) as *mut u64;
        core::ptr::write(p0, 0x1122_3344_5566_7788);
        core::ptr::write(p1, 0x8877_6655_4433_2211);
        let r0 = core::ptr::read(p0);
        let r1 = core::ptr::read(p1);
        kprintln!(
            "[test/mm][mmap][rw] p0={:#x} p1={:#x}",
            r0,
            r1
        );
        if r0 != 0x1122_3344_5566_7788 || r1 != 0x8877_6655_4433_2211 {
            let _ = do_munmap(map_addr as usize, page * 2);
            return fail("mmap", "mapped memory readback mismatch");
        }
    }

    mm_step("mmap: case=munmap_partial_second_page");
    let unmap_second = do_munmap((map_addr as usize) + page, page);
    if ret_is_err(unmap_second) {
        kprintln!(
            "[test/mm][mmap][munmap-second] errno={}",
            ret_errno(unmap_second)
        );
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "munmap second page failed");
    }

    mm_step("mmap: case=munmap_remaining_first_page");
    let unmap_first = do_munmap(map_addr as usize, page);
    if ret_is_err(unmap_first) {
        kprintln!(
            "[test/mm][mmap][munmap-first] errno={}",
            ret_errno(unmap_first)
        );
        let _ = do_munmap(map_addr as usize, page);
        return fail("mmap", "munmap first page failed");
    }

    mm_step("mmap: case=munmap_hole_idempotent");
    let unmap_hole = do_munmap(map_addr as usize, page);
    if ret_is_err(unmap_hole) {
        kprintln!(
            "[test/mm][mmap][munmap-hole] errno={}",
            ret_errno(unmap_hole)
        );
        return fail("mmap", "munmap hole should be a no-op success");
    }

    mm_step("mmap: case=invalid_munmap_zero_length");
    if expect_errno("munmap-zero-len", do_munmap(map_addr as usize, 0), syscall::EINVAL).is_err() {
        return fail("mmap", "munmap(len=0) must fail with EINVAL");
    }

    mm_step("mmap: case=invalid_munmap_unaligned");
    if expect_errno(
        "munmap-unaligned",
        do_munmap((map_addr as usize) + 1, page),
        syscall::EINVAL,
    )
    .is_err()
    {
        return fail("mmap", "munmap with unaligned address must fail");
    }

    pass("mmap");
}
