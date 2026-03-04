use super::{task_step, usermode};
use crate::fs;
use crate::mm;
use crate::syscall::{EAGAIN, EBADF, EPIPE};
use crate::{error, kprintln, ok};

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
    static __kernel_file_size: u8;
    static __kernel_mem_size: u8;
}

#[inline]
fn region_end(base: u64, size: u64) -> u64 {
    base.saturating_add(size)
}

#[inline]
fn range_contains(base: u64, end: u64, addr: u64) -> bool {
    addr >= base && addr < end
}

fn find_reserved_region_covering(addr: u64) -> Option<(u64, u64)> {
    for idx in 0..mm::memblock_reserved_region_count() {
        let Some(region) = mm::memblock_reserved_region(idx) else {
            continue;
        };
        if region.size == 0 {
            continue;
        }
        let end = region_end(region.base, region.size);
        if range_contains(region.base, end, addr) {
            return Some((region.base, end));
        }
    }

    None
}

pub(super) fn run() {
    task_step("regression: verify syscall write dispatch wiring");
    let write_ret = crate::syscall::dispatch(1, 1, 0, 1, 0, 0, 0);
    let write_errno = (-(write_ret as isize)) as i32;
    kprintln!(
        "[test/task][regression][syscall-write] ret={:#x} errno={}",
        write_ret,
        write_errno
    );
    if write_errno != crate::syscall::EFAULT {
        error!(
            "task: regression FAIL (sys_write dispatch mismatch, got errno={}, expect={})",
            write_errno,
            crate::syscall::EFAULT
        );
        return;
    }

    task_step("regression: verify kernel reserve covers bss symbols");

    let kernel_start = core::ptr::addr_of!(__kernel_start) as u64;
    let kernel_end = core::ptr::addr_of!(__kernel_end) as u64;
    let linked_file_size = core::ptr::addr_of!(__kernel_file_size) as u64;
    let linked_mem_size = core::ptr::addr_of!(__kernel_mem_size) as u64;

    let kernel_file_end = kernel_start.saturating_add(linked_file_size);
    let kernel_mem_end = kernel_start.saturating_add(linked_mem_size);
    let vmemmap_sym = core::ptr::addr_of!(mm::VMEMMAP_BASE) as u64;
    let max_pfn_sym = core::ptr::addr_of!(mm::MAX_PFN) as u64;

    kprintln!(
        "[test/task][regression][layout] kernel=[{:#x}, {:#x}) file_end={:#x} mem_end={:#x} vmemmap_sym={:#x} max_pfn_sym={:#x}",
        kernel_start,
        kernel_end,
        kernel_file_end,
        kernel_mem_end,
        vmemmap_sym,
        max_pfn_sym,
    );

    if linked_mem_size < linked_file_size {
        error!(
            "task: regression FAIL (linked_mem_size < linked_file_size: {:#x} < {:#x})",
            linked_mem_size, linked_file_size
        );
        return;
    }

    if kernel_end < kernel_mem_end {
        error!(
            "task: regression FAIL (kernel_end < kernel_mem_end: end={:#x} mem_end={:#x})",
            kernel_end, kernel_mem_end
        );
        return;
    }

    if !(vmemmap_sym >= kernel_file_end && vmemmap_sym < kernel_mem_end) {
        error!(
            "task: regression FAIL (VMEMMAP_BASE symbol not in expected bss range: sym={:#x} file_end={:#x} mem_end={:#x})",
            vmemmap_sym,
            kernel_file_end,
            kernel_mem_end,
        );
        return;
    }

    if !(max_pfn_sym >= kernel_file_end && max_pfn_sym < kernel_mem_end) {
        error!(
            "task: regression FAIL (MAX_PFN symbol not in expected bss range: sym={:#x} file_end={:#x} mem_end={:#x})",
            max_pfn_sym,
            kernel_file_end,
            kernel_mem_end,
        );
        return;
    }

    let mut kernel_range_reserved = false;
    for idx in 0..mm::memblock_reserved_region_count() {
        let Some(region) = mm::memblock_reserved_region(idx) else {
            continue;
        };
        if region.size == 0 {
            continue;
        }
        let end = region_end(region.base, region.size);
        if region.base <= kernel_start && end >= kernel_mem_end {
            kernel_range_reserved = true;
            kprintln!(
                "[test/task][regression][reserve] kernel covered by reserved region idx={} [{:#x}, {:#x})",
                idx,
                region.base,
                end,
            );
            break;
        }
    }

    if !kernel_range_reserved {
        error!(
            "task: regression FAIL (memblock reserved range does not cover full kernel mem image [{:#x}, {:#x}))",
            kernel_start,
            kernel_mem_end
        );
        return;
    }

    let Some((vmemmap_resv_base, vmemmap_resv_end)) = find_reserved_region_covering(vmemmap_sym)
    else {
        error!(
            "task: regression FAIL (VMEMMAP_BASE symbol not covered by reserved memblock region: sym={:#x})",
            vmemmap_sym
        );
        return;
    };
    let Some((max_pfn_resv_base, max_pfn_resv_end)) = find_reserved_region_covering(max_pfn_sym)
    else {
        error!(
            "task: regression FAIL (MAX_PFN symbol not covered by reserved memblock region: sym={:#x})",
            max_pfn_sym
        );
        return;
    };

    kprintln!(
        "[test/task][regression][reserve] VMEMMAP_BASE covered by [{:#x}, {:#x}), MAX_PFN covered by [{:#x}, {:#x})",
        vmemmap_resv_base,
        vmemmap_resv_end,
        max_pfn_resv_base,
        max_pfn_resv_end,
    );

    task_step("regression: verify fs static backend open/read/close");
    const REGRESSION_FS_PATH: &str = "/tests/task/fs_regression.txt";
    const REGRESSION_FS_DATA: &[u8] = b"fs-regression-ok";
    const REGRESSION_FS_PID: usize = 0xfeed;

    if let Err(errno) = fs::register_static_file(REGRESSION_FS_PATH, REGRESSION_FS_DATA) {
        error!(
            "task: regression FAIL (register_static_file errno={})",
            errno
        );
        return;
    }

    let fd = match fs::open_for_pid(REGRESSION_FS_PID, REGRESSION_FS_PATH, 0, 0) {
        Ok(fd) => fd,
        Err(errno) => {
            error!("task: regression FAIL (open_for_pid errno={})", errno);
            return;
        }
    };

    let mut buf = [0u8; 32];
    let first = match fs::read_for_pid(REGRESSION_FS_PID, fd, &mut buf[..6]) {
        Ok(n) => n,
        Err(errno) => {
            error!("task: regression FAIL (read_for_pid first errno={})", errno);
            return;
        }
    };
    let second = match fs::read_for_pid(REGRESSION_FS_PID, fd, &mut buf[6..]) {
        Ok(n) => n,
        Err(errno) => {
            error!(
                "task: regression FAIL (read_for_pid second errno={})",
                errno
            );
            return;
        }
    };
    let total = first.saturating_add(second);
    kprintln!(
        "[test/task][regression][fs] path={} fd={} first_read={} second_read={} total={}",
        REGRESSION_FS_PATH,
        fd,
        first,
        second,
        total,
    );

    if &buf[..total] != REGRESSION_FS_DATA {
        error!("task: regression FAIL (fs read content mismatch)");
        return;
    }

    if let Err(errno) = fs::close_for_pid(REGRESSION_FS_PID, fd) {
        error!("task: regression FAIL (close_for_pid errno={})", errno);
        return;
    }

    match fs::read_for_pid(REGRESSION_FS_PID, fd, &mut buf[..1]) {
        Ok(_) => {
            error!("task: regression FAIL (read should fail after close)");
            return;
        }
        Err(errno) if errno == EBADF => {}
        Err(errno) => {
            error!(
                "task: regression FAIL (unexpected errno after close, got={}, want={})",
                errno, EBADF
            );
            return;
        }
    }

    fs::drop_process_fds(REGRESSION_FS_PID);

    task_step("regression: verify syscall pipe2 dispatch wiring");
    let pipe2_ret = crate::syscall::dispatch(293, 0, 0, 0, 0, 0, 0);
    let pipe2_errno = (-(pipe2_ret as isize)) as i32;
    kprintln!(
        "[test/task][regression][syscall-pipe2] ret={:#x} errno={}",
        pipe2_ret,
        pipe2_errno
    );
    if pipe2_errno != crate::syscall::EFAULT {
        error!(
            "task: regression FAIL (sys_pipe2 dispatch mismatch, got errno={}, expect={})",
            pipe2_errno,
            crate::syscall::EFAULT
        );
        return;
    }

    task_step("regression: verify fs pipe read/write and EPIPE");
    const REGRESSION_PIPE_PID: usize = 0xbeef;
    let (rfd, wfd) = match fs::pipe2_for_pid(REGRESSION_PIPE_PID, 0) {
        Ok(v) => v,
        Err(errno) => {
            error!("task: regression FAIL (pipe2_for_pid errno={})", errno);
            return;
        }
    };
    let payload = b"pipe-regression";
    let wrote = match fs::write_for_pid(REGRESSION_PIPE_PID, wfd, payload) {
        Ok(n) => n,
        Err(errno) => {
            error!("task: regression FAIL (pipe write errno={})", errno);
            return;
        }
    };
    if wrote != payload.len() {
        error!(
            "task: regression FAIL (pipe write short, wrote={}, expect={})",
            wrote,
            payload.len()
        );
        return;
    }

    let mut pipe_buf = [0u8; 32];
    let read_n = match fs::read_for_pid(REGRESSION_PIPE_PID, rfd, &mut pipe_buf) {
        Ok(n) => n,
        Err(errno) => {
            error!("task: regression FAIL (pipe read errno={})", errno);
            return;
        }
    };
    if &pipe_buf[..read_n] != payload {
        error!("task: regression FAIL (pipe read content mismatch)");
        return;
    }

    if let Err(errno) = fs::close_for_pid(REGRESSION_PIPE_PID, rfd) {
        error!(
            "task: regression FAIL (pipe close read end errno={})",
            errno
        );
        return;
    }

    match fs::write_for_pid(REGRESSION_PIPE_PID, wfd, b"x") {
        Ok(_) => {
            error!("task: regression FAIL (pipe write should fail with EPIPE)");
            return;
        }
        Err(errno) if errno == EPIPE => {}
        Err(errno) => {
            error!(
                "task: regression FAIL (pipe write errno mismatch, got={}, want={})",
                errno, EPIPE
            );
            return;
        }
    }
    let _ = fs::close_for_pid(REGRESSION_PIPE_PID, wfd);
    fs::drop_process_fds(REGRESSION_PIPE_PID);

    task_step("regression: verify pipe2 nonblock flag and EAGAIN");
    const REGRESSION_PIPE_NB_PID: usize = 0xbeee;
    const O_NONBLOCK: u32 = 0o4000;
    let (rfd_nb, wfd_nb) = match fs::pipe2_for_pid(REGRESSION_PIPE_NB_PID, O_NONBLOCK) {
        Ok(v) => v,
        Err(errno) => {
            error!(
                "task: regression FAIL (pipe2_for_pid nonblock errno={})",
                errno
            );
            return;
        }
    };

    let mut nb_buf = [0u8; 1];
    match fs::read_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb, &mut nb_buf) {
        Ok(_) => {
            error!("task: regression FAIL (nonblock pipe read should fail with EAGAIN)");
            let _ = fs::close_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb);
            let _ = fs::close_for_pid(REGRESSION_PIPE_NB_PID, wfd_nb);
            return;
        }
        Err(errno) if errno == EAGAIN => {}
        Err(errno) => {
            error!(
                "task: regression FAIL (nonblock pipe read errno mismatch, got={}, want={})",
                errno, EAGAIN
            );
            let _ = fs::close_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb);
            let _ = fs::close_for_pid(REGRESSION_PIPE_NB_PID, wfd_nb);
            return;
        }
    }

    let _ = fs::close_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb);
    let _ = fs::close_for_pid(REGRESSION_PIPE_NB_PID, wfd_nb);
    fs::drop_process_fds(REGRESSION_PIPE_NB_PID);

    task_step("regression: run usermode exec after reserve verification");
    if !usermode::run_with_label("usermode regression") {
        error!("task: regression FAIL (usermode regression subcase failed)");
        return;
    }

    ok!("task: kernel reserve/usermode regression OK");
}
