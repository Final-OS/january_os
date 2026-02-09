//! january_os UEFI 引导程序 (x86_64)
//!
//! UEFI 引导程序负责：
//! 1. 初始化图形输出（GOP）
//! 2. 加载内核文件
//! 3. 收集硬件信息（内存映射 / ACPI / SMBIOS / 磁盘）
//! 4. 建立页表
//! 5. 退出引导服务并跳转到内核

#![no_std]
#![no_main]

use core::arch::asm;

use uefi::boot;
use uefi::prelude::*;

mod boot_services;
mod bootinfo;
mod buffers;
mod console;
mod handoff;
mod paging;
mod stages;

use buffers::allocate_boot_buffers;
use handoff::handoff_to_kernel;
use stages::{print_banner, print_handoff_stage, run_pre_exit_stages};

#[entry]
fn main() -> Status {
    print_banner();

    let buffers = allocate_boot_buffers();
    let stage = run_pre_exit_stages(&buffers);

    print_handoff_stage();

    for _ in 0..2_000_000 {
        unsafe {
            asm!("pause");
        }
    }

    let mmap = unsafe { boot::exit_boot_services(None) };

    unsafe { handoff_to_kernel(mmap, &buffers, &stage) }
}
