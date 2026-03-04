//! 引导阶段流程（退出 Boot Services 之前）

use crate::boot_services::{
    find_acpi_rsdp, find_smbios, get_runtime_services, load_kernel, scan_disks, setup_graphics,
};
use crate::bootinfo::FramebufferInfo;
use crate::buffers::BootBufferLayout;
use crate::console::{print_bool, print_dec, print_hex, print_stage, print_uefi, println_uefi};
use crate::paging::probe_paging_mode;

const KERNEL_CMDLINE: &[u8] = b"console=ttyS0 loglevel=7\0";

#[derive(Clone, Copy)]
pub struct StageState {
    pub framebuffer: FramebufferInfo,
    pub kernel_size: u64,
    pub disk_count: u32,
    pub boot_disk_index: i32,
    pub acpi_rsdp_addr: u64,
    pub acpi_version: u32,
    pub smbios_addr: u64,
    pub smbios_version: u32,
    pub runtime_services_addr: u64,
    pub cmdline_len: u32,
}

pub fn print_banner() {
    println_uefi("");
    println_uefi("  January OS Bootloader v0.1.0");
    println_uefi("");
}

pub fn run_pre_exit_stages(buffers: &BootBufferLayout) -> StageState {
    // Stage 1: Graphics
    print_stage(1, "Graphics");
    let framebuffer = setup_graphics();
    print_uefi("      ");
    print_dec(framebuffer.width as u64);
    print_uefi("x");
    print_dec(framebuffer.height as u64);
    println_uefi("");

    // Stage 2: Kernel
    print_stage(2, "Kernel");
    let kernel_size = load_kernel() as u64;
    print_uefi("      ");
    print_dec(kernel_size / 1024);
    println_uefi(" KB");

    // Stage 3: Storage
    print_stage(3, "Storage");
    let (disk_count, boot_disk_index) = scan_disks(buffers.diskinfo_phys);
    print_uefi("      ");
    print_dec(disk_count as u64);
    print_uefi(" disks, boot=");
    if boot_disk_index >= 0 {
        print_dec(boot_disk_index as u64);
    } else {
        print_uefi("N/A");
    }
    println_uefi("");

    // Stage 4: ACPI
    print_stage(4, "ACPI");
    let (acpi_rsdp_addr, acpi_version) = find_acpi_rsdp();
    if acpi_rsdp_addr != 0 {
        print_uefi("      v");
        print_dec(acpi_version as u64);
        println_uefi(".0");
    } else {
        println_uefi("      not found");
    }

    // Stage 5: SMBIOS
    print_stage(5, "SMBIOS");
    let (smbios_addr, smbios_version) = find_smbios();
    if smbios_addr != 0 {
        print_uefi("      v");
        print_dec(smbios_version as u64);
        println_uefi(".0");
    } else {
        println_uefi("      not found");
    }

    // Stage 6: Runtime Services
    print_stage(6, "Runtime");
    let runtime_services_addr = get_runtime_services();
    if runtime_services_addr != 0 {
        println_uefi("      OK");
    } else {
        println_uefi("      N/A");
    }

    // Stage 7: Page Tables
    let cmdline_len = write_kernel_cmdline(buffers.cmdline_phys);
    print_stage(7, "Page tables");
    let probe = probe_paging_mode();
    print_uefi("      LA57 probe: cpuid.7.0.ecx=");
    print_hex(probe.cpuid_7_0_ecx as u64);
    print_uefi(" cr4=");
    print_hex(probe.cr4);
    print_uefi(" supported=");
    print_bool(probe.la57_supported);
    print_uefi(" active=");
    print_bool(probe.la57_active);
    println_uefi("");
    print_uefi("      Paging mode: requested=");
    if probe.va_mode_la57_prefer {
        print_uefi("la57_prefer");
    } else {
        print_uefi("4level");
    }
    print_uefi(" fallback=");
    if probe.fallback_4level {
        print_uefi("4level");
    } else {
        print_uefi("none");
    }
    print_uefi(" selected_levels=");
    print_dec(probe.selected_page_levels as u64);
    print_uefi(" va_bits=");
    print_dec(probe.selected_va_bits as u64);
    print_uefi(" transition=");
    if probe.transition_requested {
        print_uefi("la57_trampoline");
    } else {
        print_uefi("none");
    }
    println_uefi("");

    // Stage 8: Exit Boot Services
    print_stage(8, "Exit BS");

    StageState {
        framebuffer,
        kernel_size,
        disk_count,
        boot_disk_index,
        acpi_rsdp_addr,
        acpi_version,
        smbios_addr,
        smbios_version,
        runtime_services_addr,
        cmdline_len,
    }
}

pub fn print_handoff_stage() {
    println_uefi("");
    println_uefi("  Starting kernel...");
    println_uefi("");
}

fn write_kernel_cmdline(cmdline_phys: u64) -> u32 {
    unsafe {
        let cmdline_ptr = cmdline_phys as *mut u8;
        for (index, &byte) in KERNEL_CMDLINE.iter().enumerate() {
            *cmdline_ptr.add(index) = byte;
        }
    }

    (KERNEL_CMDLINE.len() - 1) as u32
}
