//! 退出 Boot Services 后的内核交接逻辑

use core::arch::asm;

use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned};

use crate::bootinfo::{
    BootInfo, MemoryRegion, BOOTINFO_MAGIC, BOOTINFO_VERSION, DIRECT_MAP_OFFSET, KERNEL_PHYS_ADDR,
    KERNEL_STACK_PAGES, KERNEL_VIRT_ADDR,
};
use crate::buffers::BootBufferLayout;
use crate::paging::{copy_memory_map, setup_page_tables, PAGE_SIZE};
use crate::stages::StageState;

pub unsafe fn handoff_to_kernel(
    mmap: MemoryMapOwned,
    buffers: &BootBufferLayout,
    stage: &StageState,
) -> ! {
    let (mem_entries, total_mem, usable_mem, max_phys_addr) =
        copy_memory_map(mmap.entries(), buffers.memmap_phys);

    let pml4_addr = setup_page_tables(stage.kernel_size, max_phys_addr, buffers.page_table_phys);

    let boot_info_ptr = buffers.bootinfo_phys as *mut BootInfo;
    let boot_info = BootInfo {
        magic: BOOTINFO_MAGIC,
        version: BOOTINFO_VERSION,
        size: core::mem::size_of::<BootInfo>() as u32,

        framebuffer: stage.framebuffer,

        memory_map_addr: DIRECT_MAP_OFFSET + buffers.memmap_phys,
        memory_map_entries: mem_entries,
        memory_map_entry_size: core::mem::size_of::<MemoryRegion>() as u32,
        total_memory: total_mem,
        usable_memory: usable_mem,

        acpi_rsdp_addr: stage.acpi_rsdp_addr,
        acpi_version: stage.acpi_version,
        _acpi_reserved: 0,

        smbios_addr: stage.smbios_addr,
        smbios_version: stage.smbios_version,
        _smbios_reserved: 0,

        disk_info_addr: DIRECT_MAP_OFFSET + buffers.diskinfo_phys,
        disk_count: stage.disk_count,
        boot_disk_index: stage.boot_disk_index,

        uefi_runtime_services: stage.runtime_services_addr,

        kernel_phys_addr: KERNEL_PHYS_ADDR,
        kernel_virt_addr: KERNEL_VIRT_ADDR,
        kernel_size: stage.kernel_size,

        pml4_phys_addr: pml4_addr,
        direct_map_offset: DIRECT_MAP_OFFSET,

        cmdline_addr: DIRECT_MAP_OFFSET + buffers.cmdline_phys,
        cmdline_len: stage.cmdline_len,
        _cmdline_reserved: 0,
    };

    core::ptr::write_volatile(boot_info_ptr, boot_info);

    let kernel_stack_top =
        DIRECT_MAP_OFFSET + buffers.kernel_stack_phys + (KERNEL_STACK_PAGES as u64 * PAGE_SIZE) - 8;

    asm!(
        "cli",
        "mov cr3, {pml4}",
        "mov rsp, {stack}",
        "mov rdi, {boot_info}",
        "jmp {entry}",
        pml4 = in(reg) pml4_addr,
        stack = in(reg) kernel_stack_top,
        boot_info = in(reg) (DIRECT_MAP_OFFSET + buffers.bootinfo_phys),
        entry = in(reg) KERNEL_PHYS_ADDR,
        options(noreturn)
    );
}
