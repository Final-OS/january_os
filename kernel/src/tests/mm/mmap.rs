use super::{fail, mm_step, pass};
use crate::fs;
use crate::mm;
use crate::syscall;
use crate::syscall::handlers::{sys_mmap, sys_mprotect, sys_munmap};
use crate::{kprintln, warn};
use core::sync::atomic::{AtomicBool, Ordering};

const TLB_OLD_VALUE: u64 = 0x1111_2222_3333_4444;
const TLB_NEW_VALUE: u64 = 0xaaaa_bbbb_cccc_dddd;
const MMAP_FILE_PATH: &str = "/tests/mm/mmap_file.bin";
const MMAP_OFFSET_FILE_PATH: &str = "/tests/mm/mmap_offset.bin";
const MMAP_FILE_DATA: &[u8] = b"file-backed-mmap-ok";

static MMAP_ASYNC_DONE: AtomicBool = AtomicBool::new(false);

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
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][{}] expected errno={} actual=ok ret={:#x}",
                case,
                expect,
                ret
            );
        }
        return Err("expected syscall error but got success");
    }
    let got = ret_errno(ret);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][mmap][{}] expected errno={} actual_errno={}",
            case,
            expect,
            got
        );
    }
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

#[inline]
fn do_mprotect(addr: usize, len: usize, prot: u32) -> usize {
    let args = syscall::SyscallArgs::new(10, addr, len, prot as usize, 0, 0, 0);
    sys_mprotect(&args)
}

extern "C" fn mmap_run_in_task_thread() {
    run_in_task_context();
    MMAP_ASYNC_DONE.store(true, Ordering::Release);
}

pub(super) fn run() {
    if crate::task::current_pid().is_some() {
        run_in_task_context();
        return;
    }

    mm_step("mmap: no current pid, spawn task-context runner");
    MMAP_ASYNC_DONE.store(false, Ordering::Release);
    crate::task::spawn_kernel_thread("mm_mmap_runner", mmap_run_in_task_thread);

    for _ in 0..512 {
        crate::task::scheduler::schedule();
        if MMAP_ASYNC_DONE.load(Ordering::Acquire) {
            return;
        }
    }

    fail("mmap", "task-context runner timeout");
}

fn run_in_task_context() {
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

    mm_step("mmap: case=invalid_file_backed_fd");
    if expect_errno(
        "file-backed-bad-fd",
        do_mmap(0, page, prot_rw, crate::mm::mmap_flags::MAP_PRIVATE, 3, 0),
        syscall::EBADF,
    )
    .is_err()
    {
        return fail(
            "mmap",
            "file-backed mmap with invalid fd must fail with EBADF",
        );
    }

    mm_step("mmap: case=invalid_map_locked_rejected");
    if expect_errno(
        "map-locked",
        do_mmap(
            0,
            page,
            prot_rw,
            map_flags | crate::mm::mmap_flags::MAP_LOCKED,
            usize::MAX,
            0,
        ),
        syscall::EINVAL,
    )
    .is_err()
    {
        return fail("mmap", "MAP_LOCKED must be rejected until mlock is implemented");
    }

    mm_step("mmap: case=invalid_map_hugetlb_rejected");
    if expect_errno(
        "map-hugetlb",
        do_mmap(
            0,
            page,
            prot_rw,
            map_flags | crate::mm::mmap_flags::MAP_HUGETLB,
            usize::MAX,
            0,
        ),
        syscall::EINVAL,
    )
    .is_err()
    {
        return fail(
            "mmap",
            "MAP_HUGETLB must be rejected until huge-page mappings are implemented",
        );
    }

    mm_step("mmap: case=file_backed_private_read_map_and_access");
    {
        let pid = match crate::task::current_pid() {
            Some(pid) => pid.0,
            None => return fail("mmap", "file-backed mmap setup missing current pid"),
        };
        let fd = match fs::runtime::open_for_pid(pid, MMAP_FILE_PATH, 0, 0) {
            Ok(fd) => fd,
            Err(errno) => {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!("[test/mm][mmap][file-backed] open errno={}", errno);
                }
                return fail("mmap", "file-backed mmap setup open_for_pid failed");
            }
        };

        let map_ret = do_mmap(
            0,
            page,
            crate::mm::prot_flags::PROT_READ,
            crate::mm::mmap_flags::MAP_PRIVATE,
            fd as usize,
            0,
        );
        if ret_is_err(map_ret) {
            if crate::config::DEBUG_VERBOSE {
                kprintln!(
                    "[test/mm][mmap][file-backed] map errno={}",
                    ret_errno(map_ret)
                );
            }
            let _ = fs::runtime::close_for_pid(pid, fd);
            return fail("mmap", "file-backed mmap should succeed with valid fd");
        }

        let map_addr = map_ret as u64;
        unsafe {
            let p = map_addr as *const u8;
            let b0 = core::ptr::read(p);
            let b1 = core::ptr::read(p.add(1));
            if crate::config::DEBUG_VERBOSE {
                kprintln!(
                    "[test/mm][mmap][file-backed] addr={:#x} first_bytes=[{:#x}, {:#x}]",
                    map_addr,
                    b0,
                    b1
                );
            }
            if b0 != MMAP_FILE_DATA[0] || b1 != MMAP_FILE_DATA[1] {
                let _ = do_munmap(map_addr as usize, page);
                let _ = fs::runtime::close_for_pid(pid, fd);
                return fail("mmap", "file-backed mmap readback mismatch");
            }
        }

        let unmap_ret = do_munmap(map_addr as usize, page);
        if ret_is_err(unmap_ret) {
            if crate::config::DEBUG_VERBOSE {
                kprintln!(
                    "[test/mm][mmap][file-backed] munmap errno={}",
                    ret_errno(unmap_ret)
                );
            }
            let _ = fs::runtime::close_for_pid(pid, fd);
            return fail("mmap", "file-backed mmap munmap failed");
        }
        let _ = fs::runtime::close_for_pid(pid, fd);
    }

    mm_step("mmap: case=file_backed_private_nonzero_offset_read_map_and_access");
    {
        let pid = match crate::task::current_pid() {
            Some(pid) => pid.0,
            None => return fail("mmap", "file-backed offset mmap setup missing current pid"),
        };
        let fd = match fs::runtime::open_for_pid(pid, MMAP_OFFSET_FILE_PATH, 0, 0) {
            Ok(fd) => fd,
            Err(_) => return fail("mmap", "file-backed offset mmap setup open_for_pid failed"),
        };

        let map_ret = do_mmap(
            0,
            page,
            crate::mm::prot_flags::PROT_READ,
            crate::mm::mmap_flags::MAP_PRIVATE,
            fd as usize,
            page,
        );
        if ret_is_err(map_ret) {
            let _ = fs::runtime::close_for_pid(pid, fd);
            return fail("mmap", "file-backed mmap with non-zero page-aligned offset must succeed");
        }

        let map_addr = map_ret as u64;
        unsafe {
            let bytes = core::slice::from_raw_parts(map_addr as *const u8, 16);
            if bytes.iter().any(|b| *b != b'B') {
                let _ = do_munmap(map_addr as usize, page);
                let _ = fs::runtime::close_for_pid(pid, fd);
                return fail("mmap", "file-backed mmap with offset returned wrong file page");
            }
        }

        let _ = do_munmap(map_addr as usize, page);
        let _ = fs::runtime::close_for_pid(pid, fd);
    }

    mm_step("mmap: case=file_backed_munmap_split_preserves_page_offset");
    {
        let pid = match crate::task::current_pid() {
            Some(pid) => pid.0,
            None => return fail("mmap", "file-backed munmap split setup missing current pid"),
        };
        let fd = match fs::runtime::open_for_pid(pid, MMAP_OFFSET_FILE_PATH, 0, 0) {
            Ok(fd) => fd,
            Err(_) => return fail("mmap", "file-backed munmap split open_for_pid failed"),
        };

        let map_ret = do_mmap(
            0,
            page * 2,
            crate::mm::prot_flags::PROT_READ,
            crate::mm::mmap_flags::MAP_PRIVATE,
            fd as usize,
            0,
        );
        if ret_is_err(map_ret) {
            let _ = fs::runtime::close_for_pid(pid, fd);
            return fail("mmap", "two-page file-backed mmap must succeed");
        }

        let map_addr = map_ret as u64;
        unsafe {
            let first = core::ptr::read(map_addr as *const u8);
            if first != b'A' {
                let _ = do_munmap(map_addr as usize, page * 2);
                let _ = fs::runtime::close_for_pid(pid, fd);
                return fail("mmap", "file-backed base page readback mismatch before split");
            }
        }

        let unmap_ret = do_munmap(map_addr as usize, page);
        if ret_is_err(unmap_ret) {
            let _ = do_munmap(map_addr as usize + page, page);
            let _ = fs::runtime::close_for_pid(pid, fd);
            return fail("mmap", "munmap of first file-backed page failed");
        }

        unsafe {
            let bytes = core::slice::from_raw_parts((map_addr + mm::PAGE_SIZE) as *const u8, 16);
            if bytes.iter().any(|b| *b != b'B') {
                let _ = do_munmap(map_addr as usize + page, page);
                let _ = fs::runtime::close_for_pid(pid, fd);
                return fail("mmap", "munmap split lost right-side file offset after VMA rewrite");
            }
        }

        let _ = do_munmap(map_addr as usize + page, page);
        let _ = fs::runtime::close_for_pid(pid, fd);
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

    mm_step("mmap: case=mprotect_gap_failure_rolls_back_vma_flags");
    {
        let mm_ptr = crate::task::current_mm_ptr();
        if mm_ptr.is_null() {
            return fail("mmap", "mprotect rollback setup missing current mm");
        }

        let base = unsafe { &*mm_ptr }
            .find_free_area(
                low_hint.saturating_add(0x40_0000),
                mm::PAGE_SIZE * 5,
                mm::VmFlags::empty(),
            )
            .unwrap_or(low_hint.saturating_add(0x40_0000));
        let second = base.saturating_add(mm::PAGE_SIZE * 3);

        let first_map = do_mmap(
            base as usize,
            page,
            prot_rw,
            map_flags | crate::mm::mmap_flags::MAP_FIXED,
            usize::MAX,
            0,
        );
        if ret_is_err(first_map) || first_map as u64 != base {
            return fail("mmap", "mprotect rollback setup first fixed mmap failed");
        }

        let second_map = do_mmap(
            second as usize,
            page,
            prot_rw,
            map_flags | crate::mm::mmap_flags::MAP_FIXED,
            usize::MAX,
            0,
        );
        if ret_is_err(second_map) || second_map as u64 != second {
            let _ = do_munmap(base as usize, page);
            return fail("mmap", "mprotect rollback setup second fixed mmap failed");
        }

        let prot_ret = do_mprotect(
            base as usize,
            page * 3,
            crate::mm::prot_flags::PROT_READ,
        );
        if !ret_is_err(prot_ret) || ret_errno(prot_ret) != syscall::ENOMEM {
            let _ = do_munmap(base as usize, page);
            let _ = do_munmap(second as usize, page);
            return fail("mmap", "mprotect spanning an unmapped gap must fail with ENOMEM");
        }

        let first_vma = unsafe { &*mm_ptr }.find_vma(base);
        let second_vma = unsafe { &*mm_ptr }.find_vma(second);
        let first_ok = first_vma
            .as_ref()
            .map(|vma| vma.vm_start == base && vma.vm_end == base + mm::PAGE_SIZE && vma.vm_flags.is_write())
            .unwrap_or(false);
        let second_ok = second_vma
            .as_ref()
            .map(|vma| vma.vm_start == second && vma.vm_end == second + mm::PAGE_SIZE && vma.vm_flags.is_write())
            .unwrap_or(false);

        let _ = do_munmap(base as usize, page);
        let _ = do_munmap(second as usize, page);

        if !first_ok || !second_ok {
            return fail("mmap", "failed mprotect must leave surrounding VMAs and flags unchanged");
        }
    }

    mm_step("mmap: case=brk_bootstrap_heap_vma");
    {
        let mut local_mm = mm::Mm::uninit();
        let cr3 = mm::arch::read_cr3() & mm::PTE_ADDR_MASK;
        local_mm.init(cr3);
        let heap_base = mm::USER_SPACE_START.saturating_add(0x20_0000);
        let heap_new = heap_base.saturating_add(mm::PAGE_SIZE * 2);
        local_mm.start_brk = heap_base;
        local_mm.brk = heap_base;

        if local_mm.find_vma(heap_base).is_some() {
            return fail(
                "mmap",
                "brk bootstrap precondition failed: heap vma already exists",
            );
        }

        match local_mm.do_brk(heap_new) {
            Ok(actual) if actual == heap_new => {}
            Ok(actual) => {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/mm][mmap][brk-bootstrap] unexpected brk value expected={:#x} actual={:#x}",
                        heap_new,
                        actual
                    );
                }
                return fail("mmap", "do_brk returned unexpected value");
            }
            Err(e) => {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!("[test/mm][mmap][brk-bootstrap] do_brk failed err={}", e);
                }
                return fail("mmap", "do_brk should create heap vma when missing");
            }
        }

        let Some(vma) = local_mm.find_vma(heap_base) else {
            return fail("mmap", "do_brk did not create heap VMA");
        };
        let Some(flags) = local_mm.find_vma_flags(heap_base) else {
            return fail("mmap", "heap VMA flags missing after do_brk");
        };
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][brk-bootstrap] vma=[{:#x}, {:#x}) flags={:#x}",
                vma.vm_start,
                vma.vm_end,
                flags.bits()
            );
        }
        if vma.vm_start != heap_base || vma.vm_end != heap_new {
            return fail("mmap", "heap VMA range mismatch after do_brk bootstrap");
        }
        if !flags.contains(mm::VmFlags::HEAP)
            || !flags.contains(mm::VmFlags::ANONYMOUS)
            || !flags.contains(mm::VmFlags::WRITE)
            || !flags.contains(mm::VmFlags::MAYWRITE)
        {
            return fail("mmap", "heap bootstrap VMA flags incomplete");
        }
    }

    mm_step("mmap: case=stack_expand_limit_window");
    {
        let mut local_mm = mm::Mm::uninit();
        let cr3 = mm::arch::read_cr3() & mm::PTE_ADDR_MASK;
        local_mm.init(cr3);
        local_mm.start_stack = mm::USER_STACK_TOP;

        let stack_top = mm::USER_STACK_TOP;
        let stack_start = stack_top.saturating_sub(mm::PAGE_SIZE);
        let mut stack_flags = mm::VmFlags::empty();
        stack_flags.set(mm::VmFlags::READ);
        stack_flags.set(mm::VmFlags::WRITE);
        stack_flags.set(mm::VmFlags::ANONYMOUS);
        stack_flags.set(mm::VmFlags::GROWSDOWN);
        if !local_mm.insert_vma(stack_start, stack_top, mm::VmaInfo::new(stack_flags)) {
            return fail(
                "mmap",
                "stack expand regression setup failed: insert stack vma",
            );
        }

        let too_low = stack_top
            .saturating_sub(mm::USER_STACK_SIZE)
            .saturating_sub(mm::PAGE_SIZE);
        let allowed = stack_top.saturating_sub(2 * mm::PAGE_SIZE);

        if local_mm.expand_stack_for_fault(too_low).is_some() {
            return fail(
                "mmap",
                "stack expansion must reject addresses below stack size limit",
            );
        }

        let Some(expanded_vma) = local_mm.expand_stack_for_fault(allowed) else {
            return fail(
                "mmap",
                "stack expansion should succeed within stack size limit",
            );
        };
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][stack-expand] old_start={:#x} new_start={:#x} limit_bottom={:#x}",
                stack_start,
                expanded_vma.vm_start,
                stack_top.saturating_sub(mm::USER_STACK_SIZE)
            );
        }
        if expanded_vma.vm_start != allowed || expanded_vma.vm_end != stack_top {
            return fail("mmap", "stack expansion produced unexpected VMA range");
        }
    }

    mm_step("mmap: case=anon_private_rw_map_and_access");
    mm_step("mmap: case=current_mm_routing_for_mmap_and_munmap");
    {
        let Some(current_pid) = crate::task::current_pid() else {
            return fail("mmap", "current-mm routing: missing current pid");
        };
        let Some(process_ref) = crate::task::find_process_by_pid(current_pid) else {
            return fail("mmap", "current-mm routing: current process not found");
        };

        let original_mm = { process_ref.lock().mm };
        let mut local_mm = mm::Mm::uninit();
        let cr3 = mm::arch::read_cr3() & mm::PTE_ADDR_MASK;
        local_mm.init(cr3);
        let mut pt_mgr = unsafe { mm::PageTableManager::new(cr3, mm::DIRECT_MAP_OFFSET) };

        let candidate_va = {
            let init_mm = mm::get_init_mm();
            let mut probe = mm::page_align_down(mm::USER_STACK_TOP.saturating_sub(0x5000_0000));
            let mut found = 0u64;
            for _ in 0..8192 {
                if probe <= mm::USER_SPACE_START.saturating_add(mm::PAGE_SIZE) {
                    break;
                }

                let end = probe.saturating_add(mm::PAGE_SIZE);
                let init_vma_free = init_mm.find_vma_intersection(probe, end).is_none();
                let pte_free = pt_mgr.translate_addr(probe).is_none();
                if init_vma_free && pte_free {
                    found = probe;
                    break;
                }
                probe = probe.saturating_sub(mm::PAGE_SIZE);
            }
            found
        };

        if candidate_va == 0 {
            return fail(
                "mmap",
                "current-mm routing: unable to find free MAP_FIXED address",
            );
        }

        let local_mm_ptr = (&mut local_mm as *mut mm::Mm) as usize;
        let case_result = (|| -> Result<(), &'static str> {
            {
                let mut process = process_ref.lock();
                process.mm = local_mm_ptr;
            }

            if crate::task::current_mm_ptr() as usize != local_mm_ptr {
                return Err(
                    "current-mm routing: task::current_mm_ptr did not follow process.mm override",
                );
            }

            let map_ret = do_mmap(
                candidate_va as usize,
                page,
                prot_rw,
                map_flags | mm::mmap_flags::MAP_FIXED,
                usize::MAX,
                0,
            );
            if ret_is_err(map_ret) {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/mm][mmap][current-mm] mmap errno={} va={:#x}",
                        ret_errno(map_ret),
                        candidate_va
                    );
                }
                return Err("current-mm routing: mmap failed after mm switch");
            }

            if map_ret as u64 != candidate_va {
                return Err("current-mm routing: MAP_FIXED returned unexpected address");
            }
            if local_mm.find_vma(candidate_va).is_none() {
                return Err("current-mm routing: local mm missing mapped VMA");
            }
            if mm::get_init_mm()
                .find_vma_intersection(candidate_va, candidate_va.saturating_add(mm::PAGE_SIZE))
                .is_some()
            {
                return Err("current-mm routing: init_mm unexpectedly received mapped VMA");
            }

            let unmap_ret = do_munmap(candidate_va as usize, page);
            if ret_is_err(unmap_ret) {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/mm][mmap][current-mm] munmap errno={} va={:#x}",
                        ret_errno(unmap_ret),
                        candidate_va
                    );
                }
                return Err("current-mm routing: munmap failed after mm switch");
            }
            if local_mm.find_vma(candidate_va).is_some() {
                return Err("current-mm routing: local mm VMA still present after munmap");
            }

            Ok(())
        })();

        {
            let mut process = process_ref.lock();
            process.mm = original_mm;
        }

        if let Err(msg) = case_result {
            return fail("mmap", msg);
        }
    }

    let low_hint_mapped_before = {
        let cr3 = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        let pt_mgr = unsafe { crate::mm::PageTableManager::new(cr3, crate::mm::DIRECT_MAP_OFFSET) };
        pt_mgr.translate_addr(low_hint).is_some()
    };
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][mmap][precheck] low_hint={:#x} mapped_before={}",
            low_hint,
            low_hint_mapped_before
        );
    }
    let map_ret = do_mmap(0, page * 2, prot_rw, map_flags, usize::MAX, 0);
    if ret_is_err(map_ret) {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][map] expected success actual_errno={}",
                ret_errno(map_ret)
            );
        }
        return fail("mmap", "anonymous private mmap failed");
    }
    let map_addr = map_ret as u64;
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][mmap][map] addr={:#x} len={} page_size={}",
            map_addr,
            page * 2,
            page
        );
    }
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
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/mm][mmap][rw] p0={:#x} p1={:#x}", r0, r1);
        }
        if r0 != 0x1122_3344_5566_7788 || r1 != 0x8877_6655_4433_2211 {
            let _ = do_munmap(map_addr as usize, page * 2);
            return fail("mmap", "mapped memory readback mismatch");
        }
    }

    mm_step("mmap: case=map_fixed_replace_existing_mapping");
    let replace_ret = do_mmap(
        map_addr as usize,
        page,
        prot_rw,
        map_flags | crate::mm::mmap_flags::MAP_FIXED,
        usize::MAX,
        0,
    );
    if ret_is_err(replace_ret) {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][map-fixed-replace] errno={}",
                ret_errno(replace_ret)
            );
        }
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "MAP_FIXED should replace existing mapping");
    }
    if replace_ret as u64 != map_addr {
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "MAP_FIXED replace returned unexpected address");
    }

    mm_step("mmap: case=munmap_partial_second_page");
    let unmap_second = do_munmap((map_addr as usize) + page, page);
    if ret_is_err(unmap_second) {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][munmap-second] errno={}",
                ret_errno(unmap_second)
            );
        }
        let _ = do_munmap(map_addr as usize, page * 2);
        return fail("mmap", "munmap second page failed");
    }

    mm_step("mmap: case=munmap_remaining_first_page");
    let unmap_first = do_munmap(map_addr as usize, page);
    if ret_is_err(unmap_first) {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][munmap-first] errno={}",
                ret_errno(unmap_first)
            );
        }
        let _ = do_munmap(map_addr as usize, page);
        return fail("mmap", "munmap first page failed");
    }

    mm_step("mmap: case=munmap_hole_idempotent");
    let unmap_hole = do_munmap(map_addr as usize, page);
    if ret_is_err(unmap_hole) {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][mmap][munmap-hole] errno={}",
                ret_errno(unmap_hole)
            );
        }
        return fail("mmap", "munmap hole should be a no-op success");
    }

    mm_step("mmap: case=invalid_munmap_zero_length");
    if expect_errno(
        "munmap-zero-len",
        do_munmap(map_addr as usize, 0),
        syscall::EINVAL,
    )
    .is_err()
    {
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

    mm_step("mmap: case=file_fault_without_backend_returns_sigbus");
    let mut local_mm = mm::Mm::uninit();
    let cr3 = mm::arch::read_cr3() & mm::PTE_ADDR_MASK;
    local_mm.init(cr3);

    // 在用户高地址挑一个页，要求当前页表里未映射，避免误判。
    let mut probe = local_mm.mmap_base.saturating_sub(mm::PAGE_SIZE);
    let pt_mgr = unsafe { mm::PageTableManager::new(cr3, mm::DIRECT_MAP_OFFSET) };
    let mut found = None;
    for _ in 0..2048 {
        if probe < mm::USER_SPACE_START {
            break;
        }
        if pt_mgr.translate_addr(probe).is_none() {
            found = Some(probe);
            break;
        }
        probe = probe.saturating_sub(mm::PAGE_SIZE);
    }
    let Some(fault_addr) = found else {
        return fail(
            "mmap",
            "unable to find unmapped user page for file-fault regression",
        );
    };

    let vma_start = fault_addr;
    let vma_end = fault_addr.saturating_add(mm::PAGE_SIZE);
    let mut file_like_flags = mm::VmFlags::empty();
    file_like_flags.set(mm::VmFlags::READ);
    let info = mm::VmaInfo::new(file_like_flags);
    if !local_mm.insert_vma(vma_start, vma_end, info) {
        return fail("mmap", "failed to insert synthetic file-backed VMA");
    }

    let mm_ptr: *mut mm::Mm = &mut local_mm;
    let mut fault_ctx = mm::FaultContext::new(fault_addr, 0, mm_ptr, mm::DIRECT_MAP_OFFSET);
    let result = mm::handle_page_fault(&mut fault_ctx);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][mmap][file-fault] addr={:#x} result={:?} expected={:?}",
            fault_addr,
            result,
            mm::FaultResult::Sigbus
        );
    }
    if result != mm::FaultResult::Sigbus {
        return fail(
            "mmap",
            "file-backed fault fallback must return Sigbus without backend",
        );
    }

    mm_step("mmap: case=munmap_cross_cpu_visibility");
    let online_cpus = crate::smp::cpu_count();
    let detected_cpus = crate::smp::detected_cpu_count();
    if online_cpus <= 1 {
        if detected_cpus > 1 {
            return fail(
                "mmap",
                "cross-cpu munmap visibility: detected multiple CPUs but only one CPU online",
            );
        }
        warn!(
            "mm/mmap: single-cpu environment, skip cross-cpu munmap visibility check (detected={} online={})",
            detected_cpus, online_cpus,
        );
    } else {
        let cr3 = mm::arch::read_cr3() & mm::PTE_ADDR_MASK;
        let mut pt_mgr = unsafe { mm::PageTableManager::new(cr3, mm::DIRECT_MAP_OFFSET) };

        let mut probe = mm::page_align_down(mm::USER_STACK_TOP.saturating_sub(0x3000_0000));
        let test_va = {
            let mm_state = mm::get_init_mm();
            let mut found = 0u64;
            for _ in 0..8192 {
                if probe <= mm::USER_SPACE_START.saturating_add(mm::PAGE_SIZE) {
                    break;
                }
                let end = probe.saturating_add(mm::PAGE_SIZE);
                let vma_free = mm_state.find_vma_intersection(probe, end).is_none();
                let pte_free = pt_mgr.translate_addr(probe).is_none();
                if vma_free && pte_free {
                    found = probe;
                    break;
                }
                probe = probe.saturating_sub(mm::PAGE_SIZE);
            }
            found
        };
        if test_va == 0 {
            return fail(
                "mmap",
                "cross-cpu munmap visibility: unable to find free test VA",
            );
        }

        let old_page_ptr = match mm::alloc_page(mm::GFP_KERNEL_ZERO) {
            Some(page_ref) => page_ref as *mut mm::Page,
            None => return fail("mmap", "cross-cpu munmap visibility: alloc old page failed"),
        };
        let new_page_ptr = match mm::alloc_page(mm::GFP_KERNEL_ZERO) {
            Some(page_ref) => page_ref as *mut mm::Page,
            None => {
                unsafe { mm::free_page(&mut *old_page_ptr) };
                return fail("mmap", "cross-cpu munmap visibility: alloc new page failed");
            }
        };

        let old_phys = unsafe { mm::page_to_pfn(&*old_page_ptr) * mm::PAGE_SIZE };
        let new_phys = unsafe { mm::page_to_pfn(&*new_page_ptr) * mm::PAGE_SIZE };
        let pte_flags = mm::PTE_PRESENT | mm::PTE_USER | mm::PTE_WRITABLE | mm::PTE_NO_EXECUTE;

        unsafe {
            core::ptr::write_volatile(mm::phys_to_virt(old_phys) as *mut u64, TLB_OLD_VALUE);
            core::ptr::write_volatile(mm::phys_to_virt(new_phys) as *mut u64, TLB_NEW_VALUE);
        }

        if unsafe { !pt_mgr.map_page(test_va, old_phys, pte_flags) } {
            unsafe {
                mm::free_page(&mut *new_page_ptr);
                mm::free_page(&mut *old_page_ptr);
            }
            return fail(
                "mmap",
                "cross-cpu munmap visibility: initial map_page failed",
            );
        }

        let mut vma_flags = mm::VmFlags::empty();
        vma_flags.set(mm::VmFlags::READ);
        vma_flags.set(mm::VmFlags::WRITE);
        vma_flags.set(mm::VmFlags::ANONYMOUS);
        let vma_info = mm::VmaInfo::new(vma_flags);
        if !mm::get_init_mm().insert_vma(test_va, test_va.saturating_add(mm::PAGE_SIZE), vma_info) {
            unsafe {
                let _ = pt_mgr.unmap_page(test_va);
                mm::free_page(&mut *new_page_ptr);
                mm::free_page(&mut *old_page_ptr);
            }
            return fail("mmap", "cross-cpu munmap visibility: insert_vma failed");
        }

        let mut case_error: Option<&str> = None;
        let (targets_old, handled_old, matched_old) =
            mm::paging::run_tlb_probe_on_other_cpus(test_va, TLB_OLD_VALUE);
        if targets_old == 0 {
            warn!(
                "mm/mmap: cross-cpu probe found no targets (cpu_count={} registered_shootdown_cpus={})",
                crate::smp::cpu_count(),
                mm::paging::tlb_shootdown_registered_cpu_count(),
            );
            case_error = Some("cross-cpu munmap visibility: no remote probe targets");
        } else {
            if crate::config::DEBUG_VERBOSE {
                kprintln!(
                    "[test/mm][mmap][cross-cpu][pre] va={:#x} targets={} handled={} matched_old={}",
                    test_va,
                    targets_old,
                    handled_old,
                    matched_old,
                );
            }
            if handled_old != targets_old {
                case_error =
                    Some("cross-cpu munmap visibility: pre probe IPI not handled on all targets");
            } else if matched_old == 0 {
                case_error =
                    Some("cross-cpu munmap visibility: pre probe did not observe old mapping");
            }
        }

        let mut vma_present = true;

        if case_error.is_none() {
            if case_error.is_none() {
                let munmap_ret = do_munmap(test_va as usize, page);
                if ret_is_err(munmap_ret) {
                    case_error = Some("cross-cpu munmap visibility: sys_munmap failed");
                } else {
                    vma_present = false;
                }
            }

            if case_error.is_none() && unsafe { !pt_mgr.map_page(test_va, new_phys, pte_flags) } {
                case_error = Some("cross-cpu munmap visibility: remap new page failed");
            }

            if case_error.is_none() {
                let mut remap_vma_flags = mm::VmFlags::empty();
                remap_vma_flags.set(mm::VmFlags::READ);
                remap_vma_flags.set(mm::VmFlags::WRITE);
                remap_vma_flags.set(mm::VmFlags::ANONYMOUS);
                let remap_ok = mm::get_init_mm().insert_vma(
                    test_va,
                    test_va.saturating_add(mm::PAGE_SIZE),
                    mm::VmaInfo::new(remap_vma_flags),
                );
                if !remap_ok {
                    case_error = Some("cross-cpu munmap visibility: re-insert VMA failed");
                } else {
                    vma_present = true;
                }
            }

            if case_error.is_none() {
                let (targets_new, handled_new, matched_new) =
                    mm::paging::run_tlb_probe_on_other_cpus(test_va, TLB_NEW_VALUE);
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/mm][mmap][cross-cpu][post] va={:#x} targets={} handled={} matched_new={}",
                        test_va,
                        targets_new,
                        handled_new,
                        matched_new,
                    );
                }
                if targets_new == 0 {
                    case_error = Some("cross-cpu munmap visibility: no probe targets after remap");
                } else if handled_new != targets_new {
                    case_error =
                        Some("cross-cpu munmap visibility: probe IPI not handled on all targets");
                } else if matched_new != targets_new {
                    case_error = Some(
                        "cross-cpu munmap visibility: remote CPUs did not observe new mapping",
                    );
                }
            }
        }

        unsafe {
            let _ = pt_mgr.unmap_page(test_va);
        }
        if vma_present {
            let _ = mm::get_init_mm().remove_vma(test_va);
        }

        unsafe {
            // Cleanup may run after mm teardown paths already released these pages.
            // Guard refcount to avoid test-path double-free noise.
            if (*new_page_ptr).refcount() > 0 {
                mm::free_page(&mut *new_page_ptr);
            } else {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/mm][mmap][cleanup] skip free new_page (already released) pfn={}",
                        mm::page_to_pfn(&*new_page_ptr),
                    );
                }
            }
            if (*old_page_ptr).refcount() > 0 {
                mm::free_page(&mut *old_page_ptr);
            } else {
                if crate::config::DEBUG_VERBOSE {
                    kprintln!(
                        "[test/mm][mmap][cleanup] skip free old_page (already released) pfn={}",
                        mm::page_to_pfn(&*old_page_ptr),
                    );
                }
            }
        }

        if let Some(msg) = case_error {
            return fail("mmap", msg);
        }
    }

    pass("mmap");
}
