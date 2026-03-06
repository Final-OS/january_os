#![no_std]

use core::arch::asm;

pub const SYS_READ: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_OPEN: usize = 2;
pub const SYS_CLOSE: usize = 3;
pub const SYS_MMAP: usize = 9;
pub const SYS_WAIT4: usize = 61;
pub const SYS_FORK: usize = 57;
pub const SYS_GETCWD: usize = 79;
pub const SYS_CHDIR: usize = 80;
pub const SYS_GETDENTS64: usize = 217;
pub const SYS_EXECVE: usize = 59;
pub const SYS_EXIT: usize = 60;

pub const O_RDONLY: u32 = 0;
pub const PROT_READ: u32 = 0x1;
pub const PROT_WRITE: u32 = 0x2;
pub const MAP_PRIVATE: u32 = 0x02;
pub const MAP_ANONYMOUS: u32 = 0x20;

#[inline(always)]
unsafe fn raw_syscall6(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> isize {
    let mut ret = nr as isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") ret,
            in("rdi") arg0 as isize,
            in("rsi") arg1 as isize,
            in("rdx") arg2 as isize,
            in("r10") arg3 as isize,
            in("r8") arg4 as isize,
            in("r9") arg5 as isize,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
pub fn syscall3(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    unsafe { raw_syscall6(nr, arg0, arg1, arg2, 0, 0, 0) }
}

#[inline(always)]
pub fn syscall4(nr: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize) -> isize {
    unsafe { raw_syscall6(nr, arg0, arg1, arg2, arg3, 0, 0) }
}

#[inline(always)]
pub fn syscall6(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> isize {
    unsafe { raw_syscall6(nr, arg0, arg1, arg2, arg3, arg4, arg5) }
}

#[inline(always)]
pub fn syscall2(nr: usize, arg0: usize, arg1: usize) -> isize {
    unsafe { raw_syscall6(nr, arg0, arg1, 0, 0, 0, 0) }
}

#[inline(always)]
pub fn syscall1(nr: usize, arg0: usize) -> isize {
    unsafe { raw_syscall6(nr, arg0, 0, 0, 0, 0, 0) }
}

#[inline(always)]
pub fn syscall0(nr: usize) -> isize {
    unsafe { raw_syscall6(nr, 0, 0, 0, 0, 0, 0) }
}

#[inline]
pub fn read(fd: i32, buf: *mut u8, len: usize) -> isize {
    syscall3(SYS_READ, fd as usize, buf as usize, len)
}

#[inline]
pub fn write(fd: i32, buf: *const u8, len: usize) -> isize {
    syscall3(SYS_WRITE, fd as usize, buf as usize, len)
}

#[inline]
pub fn open(path: *const u8, flags: u32, mode: u16) -> isize {
    syscall3(SYS_OPEN, path as usize, flags as usize, mode as usize)
}

#[inline]
pub fn close(fd: i32) -> isize {
    syscall1(SYS_CLOSE, fd as usize)
}

#[inline]
pub fn mmap(
    addr: usize,
    len: usize,
    prot: u32,
    flags: u32,
    fd: usize,
    offset: usize,
) -> isize {
    syscall6(
        SYS_MMAP,
        addr,
        len,
        prot as usize,
        flags as usize,
        fd,
        offset,
    )
}

#[inline]
pub fn chdir(path: *const u8) -> isize {
    syscall1(SYS_CHDIR, path as usize)
}

#[inline]
pub fn getcwd(buf: *mut u8, size: usize) -> isize {
    syscall2(SYS_GETCWD, buf as usize, size)
}

#[inline]
pub fn getdents64(fd: i32, dirp: *mut u8, count: usize) -> isize {
    syscall3(SYS_GETDENTS64, fd as usize, dirp as usize, count)
}

#[inline]
pub fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> isize {
    syscall3(SYS_EXECVE, path as usize, argv as usize, envp as usize)
}

#[inline]
pub fn fork() -> isize {
    syscall0(SYS_FORK)
}

#[inline]
pub fn wait4(pid: i32, status: *mut i32, options: usize, rusage: *mut u8) -> isize {
    syscall4(
        SYS_WAIT4,
        pid as usize,
        status as usize,
        options,
        rusage as usize,
    )
}

#[inline]
pub fn exit(code: i32) -> ! {
    let _ = syscall1(SYS_EXIT, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

#[inline]
pub fn errno_from_ret(ret: isize) -> i32 {
    if ret < 0 {
        (-ret) as i32
    } else {
        0
    }
}

#[inline]
pub fn write_str(text: &str) {
    let _ = write(1, text.as_ptr(), text.len());
}

#[inline]
pub fn write_line(text: &str) {
    write_str(text);
    write_str("\n");
}

#[inline]
pub fn write_u64(mut value: u64) {
    let mut buf = [0u8; 20];
    if value == 0 {
        let _ = write(1, b"0".as_ptr(), 1);
        return;
    }
    let mut len = 0usize;
    while value > 0 {
        buf[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    for idx in (0..len).rev() {
        let _ = write(1, (&buf[idx]) as *const u8, 1);
    }
}

#[inline]
pub fn with_cstr<R>(text: &str, scratch: &mut [u8], f: impl FnOnce(*const u8) -> R) -> Option<R> {
    if text.len().saturating_add(1) > scratch.len() {
        return None;
    }
    scratch[..text.len()].copy_from_slice(text.as_bytes());
    scratch[text.len()] = 0;
    Some(f(scratch.as_ptr()))
}

#[inline]
pub unsafe fn argc_argv() -> (usize, *const usize) {
    let rsp: usize;
    unsafe {
        asm!("mov {}, rsp", out(reg) rsp, options(nostack, preserves_flags));
    }
    let stack = rsp as *const usize;
    let argc = unsafe { *stack };
    let argv = unsafe { stack.add(1) };
    (argc, argv)
}

#[inline]
pub unsafe fn argv_at(argv: *const usize, index: usize) -> Option<&'static str> {
    let ptr = unsafe { *argv.add(index) } as *const u8;
    if ptr.is_null() {
        return None;
    }
    unsafe { cstr_to_str(ptr) }
}

#[inline]
pub unsafe fn cstr_to_str(ptr: *const u8) -> Option<&'static str> {
    let mut len = 0usize;
    while len < 4096 {
        let byte = unsafe { *ptr.add(len) };
        if byte == 0 {
            break;
        }
        len += 1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(bytes).ok()
}
