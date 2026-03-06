use super::{task_step, usermode};
use crate::fs;
use crate::mm;
use crate::syscall::{EAGAIN, EBADF, EPIPE};
use crate::{error, kprintln, ok};

unsafe extern "C" {
    static __kernel_phys_base: u8;
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

    let kernel_phys_start = core::ptr::addr_of!(__kernel_phys_base) as u64;
    let kernel_virt_start = core::ptr::addr_of!(__kernel_start) as u64;
    let kernel_virt_end = core::ptr::addr_of!(__kernel_end) as u64;
    let linked_file_size = core::ptr::addr_of!(__kernel_file_size) as u64;
    let linked_mem_size = core::ptr::addr_of!(__kernel_mem_size) as u64;

    let kernel_file_end = kernel_virt_start.saturating_add(linked_file_size);
    let kernel_mem_end = kernel_virt_start.saturating_add(linked_mem_size);
    let kernel_phys_end = kernel_phys_start.saturating_add(linked_mem_size);
    let vmemmap_sym = core::ptr::addr_of!(mm::VMEMMAP_BASE) as u64;
    let max_pfn_sym = core::ptr::addr_of!(mm::MAX_PFN) as u64;
    let kernel_slide = kernel_virt_start.saturating_sub(kernel_phys_start);
    let vmemmap_phys = vmemmap_sym.saturating_sub(kernel_slide);
    let max_pfn_phys = max_pfn_sym.saturating_sub(kernel_slide);

    kprintln!(
        "[test/task][regression][layout] kernel_phys=[{:#x}, {:#x}) kernel_virt=[{:#x}, {:#x}) file_end={:#x} mem_end={:#x} vmemmap_sym={:#x} max_pfn_sym={:#x}",
        kernel_phys_start,
        kernel_phys_end,
        kernel_virt_start,
        kernel_virt_end,
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

    if kernel_virt_end < kernel_mem_end {
        error!(
            "task: regression FAIL (kernel_end < kernel_mem_end: end={:#x} mem_end={:#x})",
            kernel_virt_end, kernel_mem_end
        );
        return;
    }

    if !(vmemmap_sym >= kernel_file_end && vmemmap_sym < kernel_mem_end) {
        error!(
            "task: regression FAIL (VMEMMAP_BASE symbol not in expected bss range: sym={:#x} file_end={:#x} mem_end={:#x})",
            vmemmap_sym, kernel_file_end, kernel_mem_end,
        );
        return;
    }

    if !(max_pfn_sym >= kernel_file_end && max_pfn_sym < kernel_mem_end) {
        error!(
            "task: regression FAIL (MAX_PFN symbol not in expected bss range: sym={:#x} file_end={:#x} mem_end={:#x})",
            max_pfn_sym, kernel_file_end, kernel_mem_end,
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
        if region.base <= kernel_phys_start && end >= kernel_phys_end {
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
            kernel_phys_start, kernel_phys_end
        );
        return;
    }

    let Some((vmemmap_resv_base, vmemmap_resv_end)) = find_reserved_region_covering(vmemmap_phys)
    else {
        error!(
            "task: regression FAIL (VMEMMAP_BASE symbol not covered by reserved memblock region: sym={:#x} phys={:#x})",
            vmemmap_sym,
            vmemmap_phys
        );
        return;
    };
    let Some((max_pfn_resv_base, max_pfn_resv_end)) = find_reserved_region_covering(max_pfn_phys)
    else {
        error!(
            "task: regression FAIL (MAX_PFN symbol not covered by reserved memblock region: sym={:#x} phys={:#x})",
            max_pfn_sym,
            max_pfn_phys
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

    task_step("regression: verify fs initramfs backend open/read/close");
    const REGRESSION_FS_PATH: &str = "/tests/task/fs_regression.txt";
    const REGRESSION_FS_DATA: &[u8] = b"fs-regression-ok";
    const REGRESSION_FS_PID: usize = 0xfeed;

    let fd = match fs::runtime::open_for_pid(REGRESSION_FS_PID, REGRESSION_FS_PATH, 0, 0) {
        Ok(fd) => fd,
        Err(errno) => {
            error!("task: regression FAIL (open_for_pid errno={})", errno);
            return;
        }
    };

    let mut buf = [0u8; 32];
    let first = match fs::runtime::read_for_pid(REGRESSION_FS_PID, fd, &mut buf[..6]) {
        Ok(n) => n,
        Err(errno) => {
            error!("task: regression FAIL (read_for_pid first errno={})", errno);
            return;
        }
    };
    let second = match fs::runtime::read_for_pid(REGRESSION_FS_PID, fd, &mut buf[6..]) {
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

    if let Err(errno) = fs::runtime::close_for_pid(REGRESSION_FS_PID, fd) {
        error!("task: regression FAIL (close_for_pid errno={})", errno);
        return;
    }

    match fs::runtime::read_for_pid(REGRESSION_FS_PID, fd, &mut buf[..1]) {
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

    fs::runtime::drop_process_fds(REGRESSION_FS_PID);

    task_step("regression: verify fs lseek/dup/chdir/getcwd/readdir helpers");
    let fd = match fs::runtime::open_for_pid(REGRESSION_FS_PID, REGRESSION_FS_PATH, 0, 0) {
        Ok(fd) => fd,
        Err(errno) => {
            error!("task: regression FAIL (open_for_pid #2 errno={})", errno);
            return;
        }
    };
    let pos = match fs::runtime::lseek_for_pid(REGRESSION_FS_PID, fd, 3, 0) {
        Ok(v) => v,
        Err(errno) => {
            error!("task: regression FAIL (lseek_for_pid errno={})", errno);
            return;
        }
    };
    if pos != 3 {
        error!("task: regression FAIL (lseek position mismatch)");
        return;
    }
    let mut tail = [0u8; 32];
    let tail_n = match fs::runtime::read_for_pid(REGRESSION_FS_PID, fd, &mut tail) {
        Ok(v) => v,
        Err(errno) => {
            error!("task: regression FAIL (read after lseek errno={})", errno);
            return;
        }
    };
    if &tail[..tail_n] != &REGRESSION_FS_DATA[3..] {
        error!("task: regression FAIL (tail read mismatch)");
        return;
    }

    let dupfd = match fs::runtime::dup_for_pid(REGRESSION_FS_PID, fd, 0, false) {
        Ok(v) => v,
        Err(errno) => {
            error!("task: regression FAIL (dup_for_pid errno={})", errno);
            return;
        }
    };
    if let Err(errno) = fs::runtime::close_for_pid(REGRESSION_FS_PID, fd) {
        error!("task: regression FAIL (close original fd errno={})", errno);
        return;
    }
    let mut dup_buf = [0u8; 4];
    let dup_n = match fs::runtime::read_for_pid(REGRESSION_FS_PID, dupfd, &mut dup_buf) {
        Ok(v) => v,
        Err(errno) => {
            error!("task: regression FAIL (read dup fd errno={})", errno);
            return;
        }
    };
    if dup_n != 0 {
        error!("task: regression FAIL (dup fd should share offset and hit EOF)");
        return;
    }
    let _ = fs::runtime::close_for_pid(REGRESSION_FS_PID, dupfd);

    if let Err(errno) = fs::runtime::chdir_for_pid(REGRESSION_FS_PID, "/tests") {
        error!("task: regression FAIL (chdir /tests errno={})", errno);
        return;
    }
    let cwd = fs::runtime::getcwd_for_pid(REGRESSION_FS_PID);
    if cwd.as_str() != "/tests" {
        error!("task: regression FAIL (getcwd mismatch after chdir)");
        return;
    }
    let fd_rel = match fs::runtime::open_for_pid(REGRESSION_FS_PID, "task/fs_regression.txt", 0, 0) {
        Ok(fd) => fd,
        Err(errno) => {
            error!("task: regression FAIL (relative open errno={})", errno);
            return;
        }
    };
    let _ = fs::runtime::close_for_pid(REGRESSION_FS_PID, fd_rel);

    let dirfd = match fs::runtime::open_for_pid(REGRESSION_FS_PID, "/", 0, 0) {
        Ok(fd) => fd,
        Err(errno) => {
            error!("task: regression FAIL (open root dir errno={})", errno);
            return;
        }
    };
    let mut saw_tests = false;
    for _ in 0..16 {
        let entry = match fs::runtime::peek_dir_entry_for_pid(REGRESSION_FS_PID, dirfd) {
            Ok(v) => v,
            Err(errno) => {
                error!("task: regression FAIL (peek_dir_entry errno={})", errno);
                return;
            }
        };
        let Some(entry) = entry else {
            break;
        };
        if entry.name.as_str() == "tests" {
            saw_tests = true;
        }
        if let Err(errno) = fs::runtime::advance_dir_cursor_for_pid(REGRESSION_FS_PID, dirfd, 1) {
            error!("task: regression FAIL (advance_dir_cursor errno={})", errno);
            return;
        }
    }
    if !saw_tests {
        error!("task: regression FAIL (root readdir did not expose /tests)");
        return;
    }
    let _ = fs::runtime::close_for_pid(REGRESSION_FS_PID, dirfd);
    fs::runtime::drop_process_fds(REGRESSION_FS_PID);

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

    task_step("regression: verify newly wired fs syscall dispatchers");
    let lseek_ret = crate::syscall::dispatch(8, usize::MAX, 0, 0, 0, 0, 0);
    let lseek_errno = (-(lseek_ret as isize)) as i32;
    if lseek_errno != crate::syscall::EBADF {
        error!(
            "task: regression FAIL (sys_lseek dispatch mismatch, got errno={}, expect={})",
            lseek_errno,
            crate::syscall::EBADF
        );
        return;
    }
    let getcwd_ret = crate::syscall::dispatch(79, 0, 16, 0, 0, 0, 0);
    let getcwd_errno = (-(getcwd_ret as isize)) as i32;
    if getcwd_errno != crate::syscall::EFAULT {
        error!(
            "task: regression FAIL (sys_getcwd dispatch mismatch, got errno={}, expect={})",
            getcwd_errno,
            crate::syscall::EFAULT
        );
        return;
    }
    let getdents_ret = crate::syscall::dispatch(217, usize::MAX, 0, 16, 0, 0, 0);
    let getdents_errno = (-(getdents_ret as isize)) as i32;
    if getdents_errno != crate::syscall::EBADF {
        error!(
            "task: regression FAIL (sys_getdents64 dispatch mismatch, got errno={}, expect={})",
            getdents_errno,
            crate::syscall::EBADF
        );
        return;
    }

    task_step("regression: verify fs pipe read/write and EPIPE");
    const REGRESSION_PIPE_PID: usize = 0xbeef;
    let (rfd, wfd) = match fs::runtime::pipe2_for_pid(REGRESSION_PIPE_PID, 0) {
        Ok(v) => v,
        Err(errno) => {
            error!("task: regression FAIL (pipe2_for_pid errno={})", errno);
            return;
        }
    };
    let payload = b"pipe-regression";
    let wrote = match fs::runtime::write_for_pid(REGRESSION_PIPE_PID, wfd, payload) {
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
    let read_n = match fs::runtime::read_for_pid(REGRESSION_PIPE_PID, rfd, &mut pipe_buf) {
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

    if let Err(errno) = fs::runtime::close_for_pid(REGRESSION_PIPE_PID, rfd) {
        error!(
            "task: regression FAIL (pipe close read end errno={})",
            errno
        );
        return;
    }

    match fs::runtime::write_for_pid(REGRESSION_PIPE_PID, wfd, b"x") {
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
    let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_PID, wfd);
    fs::runtime::drop_process_fds(REGRESSION_PIPE_PID);

    task_step("regression: verify pipe2 nonblock flag and EAGAIN");
    const REGRESSION_PIPE_NB_PID: usize = 0xbeee;
    const O_NONBLOCK: u32 = 0o4000;
    let (rfd_nb, wfd_nb) = match fs::runtime::pipe2_for_pid(REGRESSION_PIPE_NB_PID, O_NONBLOCK) {
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
    match fs::runtime::read_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb, &mut nb_buf) {
        Ok(_) => {
            error!("task: regression FAIL (nonblock pipe read should fail with EAGAIN)");
            let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb);
            let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_NB_PID, wfd_nb);
            return;
        }
        Err(errno) if errno == EAGAIN => {}
        Err(errno) => {
            error!(
                "task: regression FAIL (nonblock pipe read errno mismatch, got={}, want={})",
                errno, EAGAIN
            );
            let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb);
            let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_NB_PID, wfd_nb);
            return;
        }
    }

    let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_NB_PID, rfd_nb);
    let _ = fs::runtime::close_for_pid(REGRESSION_PIPE_NB_PID, wfd_nb);
    fs::runtime::drop_process_fds(REGRESSION_PIPE_NB_PID);

    task_step("regression: run usermode exec after reserve verification");
    if !usermode::run_with_label("usermode regression") {
        error!("task: regression FAIL (usermode regression subcase failed)");
        return;
    }

    ok!("task: kernel reserve/usermode regression OK");
}
