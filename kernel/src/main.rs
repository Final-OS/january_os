//! january_os 内核 (x86_64)
//!
//! 这是内核的入口点，从 UEFI 引导程序接收完整的系统信息。

#![no_std]
#![no_main]
#![allow(unsafe_op_in_unsafe_fn)]
// 开发阶段允许的警告（后续逐步修复）
#![allow(dead_code)]
#![allow(unused)]
#![allow(private_interfaces)]
#![allow(non_camel_case_types)]
#![allow(mismatched_lifetime_syntaxes)]
#![allow(clippy::all)]
#![feature(alloc_error_handler)]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]

extern crate alloc;

use core::panic::PanicInfo;

// 自动生成的配置
mod generated;
pub mod config {
    pub use super::generated::*;
}

// 导入内核库模块
mod arch;
mod component;
mod drivers;
mod errno;
mod error;
mod fs;
mod interrupt;
mod libs;
mod log;
mod mm;
mod net;
mod security;
mod smp;
mod sync;
mod syscall;
mod task;
mod tests;
mod common;
mod virt;

// 新增模块
mod boot;
mod init;
mod shell;

use crate::arch::halt;
use boot::BootInfo;

// ============================================================================
// 内核入口点
// ============================================================================

// 链接脚本定义的 BSS 段边界
unsafe extern "C" {
    static __bss_start: u8;
    static __bss_end: u8;
}

extern "C" fn runtime_entry(init_cmd_ptr: *const u8, init_cmd_len: usize) -> ! {
    let init_cmd = unsafe {
        let bytes = core::slice::from_raw_parts(init_cmd_ptr, init_cmd_len);
        core::str::from_utf8_unchecked(bytes)
    };
    // `ksh` 进入内核调试 Shell；其他 init 路径由 supervisor 线程拉起并监管。
    shell::run(init_cmd);
}

/// 清零 BSS 段
///
/// # Safety
/// 必须在使用任何静态变量之前调用
#[inline(never)]
unsafe fn zero_bss() {
    let start = core::ptr::addr_of!(__bss_start) as *mut u8;
    let end = core::ptr::addr_of!(__bss_end) as *mut u8;
    let size = end as usize - start as usize;
    core::ptr::write_bytes(start, 0, size);
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
pub unsafe extern "C" fn _start(boot_info_ptr: *const BootInfo) -> ! {
    // 必须首先清零 BSS 段
    zero_bss();

    // 验证 BootInfo 指针
    if boot_info_ptr.is_null() {
        // 由于此时还没初始化串口，无法打印，只能死循环
        loop {
            halt()
        }
    }
    let info = &*boot_info_ptr;

    // 初始化内核
    init::init_kernel(info);

    // 切换到内核自有启动栈，避免后续调度依赖引导器栈映射。
    arch::switch_to_runtime_boot_stack(runtime_entry, info.initrd_command());
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 立即禁用中断，防止重入和重启
    interrupt::disable_interrupts();

    // 防止 panic 重入（嵌套 panic 直接死循环）
    use core::sync::atomic::{AtomicBool, Ordering};
    static PANICKING: AtomicBool = AtomicBool::new(false);
    if PANICKING.swap(true, Ordering::SeqCst) {
        loop {
            halt()
        }
    }

    crate::kprintln!();
    crate::kprintln!("!!! KERNEL PANIC !!!");
    if let Some(loc) = info.location() {
        crate::kprintln!("  {}:{}", loc.file(), loc.line());
    }
    crate::kprintln!("  {}", info.message());
    loop {
        halt();
    }
}
