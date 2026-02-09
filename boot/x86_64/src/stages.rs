//! 引导阶段流程（退出 Boot Services 之前）

use crate::boot_services::{
    find_acpi_rsdp, find_smbios, get_runtime_services, load_kernel, scan_disks, setup_graphics,
};
use crate::bootinfo::{FramebufferInfo, DIRECT_MAP_OFFSET, KERNEL_VIRT_ADDR};
use crate::buffers::BootBufferLayout;
use crate::console::{print_dec, print_hex, print_stage, print_uefi, println_uefi};

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
    println_uefi("========================================");
    println_uefi("  january_os UEFI Bootloader v0.1.0");
    println_uefi("  Architecture: x86_64");
    println_uefi("========================================");
    println_uefi("");
}

pub fn run_pre_exit_stages(buffers: &BootBufferLayout) -> StageState {
    print_stage(1, "Initializing graphics (GOP)...");
    let framebuffer = setup_graphics();
    print_uefi("      Resolution: ");
    print_dec(framebuffer.width as u64);
    print_uefi("x");
    print_dec(framebuffer.height as u64);
    println_uefi("");

    print_stage(2, "Loading kernel...");
    println_uefi("      Opening filesystem...");
    let kernel_size = load_kernel() as u64;
    print_uefi("      Kernel size: ");
    print_dec(kernel_size);
    println_uefi(" bytes");

    print_stage(3, "Scanning storage devices...");
    let (disk_count, boot_disk_index) = scan_disks(buffers.diskinfo_phys);
    print_uefi("      Found ");
    print_dec(disk_count as u64);
    println_uefi(" disk(s)");

    print_stage(4, "Locating ACPI tables...");
    let (acpi_rsdp_addr, acpi_version) = find_acpi_rsdp();
    if acpi_rsdp_addr != 0 {
        print_uefi("      RSDP at ");
        print_uefi("0x");
        print_hex(acpi_rsdp_addr);
        print_uefi(" (ACPI ");
        print_dec(acpi_version as u64);
        println_uefi(".0)");
    } else {
        println_uefi("      ACPI not found!");
    }

    print_stage(5, "Locating SMBIOS...");
    let (smbios_addr, smbios_version) = find_smbios();
    if smbios_addr != 0 {
        print_uefi("      SMBIOS at ");
        print_uefi("0x");
        print_hex(smbios_addr);
        println_uefi("");
    } else {
        println_uefi("      SMBIOS not found");
    }

    print_stage(6, "Getting UEFI Runtime Services...");
    let runtime_services_addr = get_runtime_services();
    if runtime_services_addr != 0 {
        print_uefi("      Runtime Services at ");
        print_uefi("0x");
        print_hex(runtime_services_addr);
        println_uefi("");
    } else {
        println_uefi("      Runtime Services not available");
    }

    let cmdline_len = write_kernel_cmdline(buffers.cmdline_phys);

    print_stage(7, "Preparing page-table handoff...");
    print_uefi("      Kernel virtual address: ");
    print_uefi("0x");
    print_hex(KERNEL_VIRT_ADDR);
    println_uefi("");
    print_uefi("      Direct map offset: ");
    print_uefi("0x");
    print_hex(DIRECT_MAP_OFFSET);
    println_uefi("");
    println_uefi("      Page tables will be finalized after ExitBootServices");

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
    print_stage(8, "Exiting boot services...");
    println_uefi("");
    println_uefi("Jumping to kernel...");
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
