use super::{fail, mm_step, pass};
use crate::{kprintln, warn};
use crate::mm;
use crate::syscall;
use crate::syscall::handlers::{sys_mmap, sys_munmap};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

const TLB_PHASE_IDLE: usize = 0;
const TLB_PHASE_READ_OLD: usize = 1;
const TLB_PHASE_PAUSE: usize = 2;
const TLB_PHASE_READ_NEW: usize = 3;
const TLB_PHASE_STOP: usize = 4;
const TLB_OLD_VALUE: u64 = 0x1111_2222_3333_4444;
const TLB_NEW_VALUE: u64 = 0xaaaa_bbbb_cccc_dddd;

static TLB_TEST_ADDR: AtomicU64 = AtomicU64::new(0);
static TLB_TEST_PHASE: AtomicUsize = AtomicUsize::new(TLB_PHASE_IDLE);
static TLB_TEST_OWNER_APIC: AtomicUsize = AtomicUsize::new(usize::MAX);
static TLB_TEST_REMOTE_SEEN: AtomicBool = AtomicBool::new(false);
static TLB_TEST_PAUSE_ACK: AtomicUsize = AtomicUsize::new(0);
static TLB_TEST_OLD_READS: AtomicUsize = AtomicUsize::new(0);
static TLB_TEST_NEW_READS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn mmap_tlb_worker() {
    loop {
        let phase = TLB_TEST_PHASE.load(Ordering::Acquire);
        if phase == TLB_PHASE_STOP {
            crate::task::exit_current_task(0);
            loop {
                crate::task::scheduler::schedule();
            }
        }

        let local_apic = crate::interrupt::local_apic_id() as usize;
        let owner_apic = TLB_TEST_OWNER_APIC.load(Ordering::Acquire);
        if owner_apic != usize::MAX && local_apic != owner_apic {
            TLB_TEST_REMOTE_SEEN.store(true, Ordering::Release);
        }

        let addr = TLB_TEST_ADDR.load(Ordering::Acquire);
        if addr != 0 {
            if phase == TLB_PHASE_READ_OLD {
                let value = unsafe { core::ptr::read_volatile(addr as *const u64) };
                if value == TLB_OLD_VALUE {
                    TLB_TEST_OLD_READS.fetch_add(1, Ordering::AcqRel);
                }
            } else if phase == TLB_PHASE_PAUSE {
                TLB_TEST_PAUSE_ACK.fetch_add(1, Ordering::AcqRel);
            } else if phase == TLB_PHASE_READ_NEW {
                let value = unsafe { core::ptr::read_volatile(addr as *const u64) };
                if value == TLB_NEW_VALUE {
                    TLB_TEST_NEW_READS.fetch_add(1, Ordering::AcqRel);
                }
            }
        }

        crate::task::scheduler::schedule();
    }
}

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
        return fail("mmap", "unable to find unmapped user page for file-fault regression");
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
    kprintln!(
        "[test/mm][mmap][file-fault] addr={:#x} result={:?} expected={:?}",
        fault_addr,
        result,
        mm::FaultResult::Sigbus
    );
    if result != mm::FaultResult::Sigbus {
        return fail("mmap", "file-backed fault fallback must return Sigbus without backend");
    }

    mm_step("mmap: case=munmap_cross_cpu_visibility");
    if crate::smp::cpu_count() <= 1 {
        warn!("mm/mmap: single-cpu environment, skip cross-cpu munmap visibility check");
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
            return fail("mmap", "cross-cpu munmap visibility: initial map_page failed");
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

        TLB_TEST_ADDR.store(test_va, Ordering::Release);
        TLB_TEST_PHASE.store(TLB_PHASE_READ_OLD, Ordering::Release);
        TLB_TEST_OWNER_APIC.store(crate::interrupt::local_apic_id() as usize, Ordering::Release);
        TLB_TEST_REMOTE_SEEN.store(false, Ordering::Release);
        TLB_TEST_PAUSE_ACK.store(0, Ordering::Release);
        TLB_TEST_OLD_READS.store(0, Ordering::Release);
        TLB_TEST_NEW_READS.store(0, Ordering::Release);

        let worker = crate::task::spawn_kernel_thread("mm_mmap_tlb_worker", mmap_tlb_worker);
        let worker_pid = worker.lock().pid;

        let mut remote_ready = false;
        for _ in 0..4096 {
            if TLB_TEST_REMOTE_SEEN.load(Ordering::Acquire)
                && TLB_TEST_OLD_READS.load(Ordering::Acquire) > 0
            {
                remote_ready = true;
                break;
            }
            crate::task::scheduler::schedule();
        }

        let mut case_error: Option<&str> = None;
        let mut munmap_done = false;
        let mut old_extra_ref_held = false;
        let mut vma_present = true;

        if !remote_ready {
            warn!(
                "mm/mmap: cross-cpu munmap visibility skipped (no remote CPU observed old mapping)"
            );
        } else {
            TLB_TEST_PHASE.store(TLB_PHASE_PAUSE, Ordering::Release);
            for _ in 0..512 {
                if TLB_TEST_PAUSE_ACK.load(Ordering::Acquire) > 0 {
                    break;
                }
                crate::task::scheduler::schedule();
            }
            if TLB_TEST_PAUSE_ACK.load(Ordering::Acquire) == 0 {
                case_error = Some("cross-cpu munmap visibility: worker pause handshake timeout");
            }

            if case_error.is_none() {
                unsafe {
                    (&*old_page_ptr).get();
                }
                old_extra_ref_held = true;

                let munmap_ret = do_munmap(test_va as usize, page);
                if ret_is_err(munmap_ret) {
                    case_error = Some("cross-cpu munmap visibility: sys_munmap failed");
                } else {
                    munmap_done = true;
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
                TLB_TEST_PHASE.store(TLB_PHASE_READ_NEW, Ordering::Release);
                let mut new_seen = false;
                for _ in 0..4096 {
                    if TLB_TEST_NEW_READS.load(Ordering::Acquire) > 0 {
                        new_seen = true;
                        break;
                    }
                    crate::task::scheduler::schedule();
                }
                kprintln!(
                    "[test/mm][mmap][cross-cpu] va={:#x} old_reads={} new_reads={} remote_seen={}",
                    test_va,
                    TLB_TEST_OLD_READS.load(Ordering::Acquire),
                    TLB_TEST_NEW_READS.load(Ordering::Acquire),
                    TLB_TEST_REMOTE_SEEN.load(Ordering::Acquire),
                );
                if !new_seen {
                    case_error = Some("cross-cpu munmap visibility: remote CPU did not observe new mapping");
                }
            }
        }

        TLB_TEST_PHASE.store(TLB_PHASE_STOP, Ordering::Release);
        let mut reaped_worker = false;
        for _ in 0..1024 {
            if let Some((pid, _code)) = crate::task::wait_child(Some(worker_pid)) {
                if pid == worker_pid {
                    reaped_worker = true;
                    break;
                }
            }
            crate::task::scheduler::schedule();
        }
        if !reaped_worker {
            warn!("mm/mmap: worker reap timeout in cross-cpu visibility case");
        }

        TLB_TEST_ADDR.store(0, Ordering::Release);
        TLB_TEST_PHASE.store(TLB_PHASE_IDLE, Ordering::Release);
        TLB_TEST_OWNER_APIC.store(usize::MAX, Ordering::Release);

        unsafe {
            let _ = pt_mgr.unmap_page(test_va);
        }
        if vma_present {
            let _ = mm::get_init_mm().remove_vma(test_va);
        }

        unsafe {
            mm::free_page(&mut *new_page_ptr);
            if old_extra_ref_held && !munmap_done {
                mm::free_page(&mut *old_page_ptr);
            }
            mm::free_page(&mut *old_page_ptr);
        }

        if let Some(msg) = case_error {
            return fail("mmap", msg);
        }
    }

    pass("mmap");
}
