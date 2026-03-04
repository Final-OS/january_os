//! 退出 Boot Services 后的内核交接逻辑

use core::arch::asm;

use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned};

use crate::bootinfo::{
    BOOTINFO_MAGIC, BOOTINFO_VERSION, BootInfo, DIRECT_MAP_OFFSET, KERNEL_PHYS_ADDR,
    KERNEL_STACK_PAGES, KERNEL_VIRT_ADDR, KernelVaLayout, MemoryRegion,
};
use crate::buffers::BootBufferLayout;
use crate::paging::{PAGE_SIZE, copy_memory_map, enter_kernel_with_la57_fallback, setup_page_tables};
use crate::stages::StageState;
use crate::cfg::{VMALLOC_END, VMALLOC_START};

pub unsafe fn handoff_to_kernel(
    mmap: MemoryMapOwned,
    buffers: &BootBufferLayout,
    stage: &StageState,
) -> ! {
    let (mem_entries, total_mem, usable_mem, max_phys_addr) =
        unsafe { copy_memory_map(mmap.entries(), buffers.memmap_phys) };

    let paging =
        unsafe { setup_page_tables(stage.kernel_size, max_phys_addr, buffers.page_table_phys) };

    let mut final_root_phys = paging.root_phys_addr;
    let mut final_pml4_compat = paging.pml4_compat_phys;
    let mut final_page_levels = paging.page_levels;
    let mut final_va_bits = paging.va_bits;

    if paging.la57_transition_root_phys != 0 {
        final_root_phys = paging.la57_transition_root_phys;
        final_pml4_compat = paging.la57_transition_pml4_compat_phys;
        final_page_levels = 5;
        final_va_bits = 57;
    }

    let kernel_layout = KernelVaLayout {
        va_bits: final_va_bits,
        page_levels: final_page_levels,
        _reserved0: [0; 6],
        direct_map_start: DIRECT_MAP_OFFSET,
        direct_map_end: paging.direct_map_window_end,
        vmalloc_start: VMALLOC_START,
        vmalloc_end: VMALLOC_END.saturating_add(1),
        // Linux 风格窗口：当前先固定默认值，后续再接入可配置化。
        vmemmap_start: 0xFFFF_EA00_0000_0000,
        vmemmap_end: 0xFFFF_EFFF_FFFF_FFFFu64.saturating_add(1),
        modules_start: 0xFFFF_FFFF_A000_0000,
        modules_end: 0xFFFF_FFFF_FEFF_FFFFu64.saturating_add(1),
        fixmap_start: 0xFFFF_FFFF_FF00_0000,
        fixmap_end: 0xFFFF_FFFF_FFFF_F000u64.saturating_add(1),
    };

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

        pml4_phys_addr: final_pml4_compat,
        direct_map_offset: DIRECT_MAP_OFFSET,
        kernel_layout,

        cmdline_addr: DIRECT_MAP_OFFSET + buffers.cmdline_phys,
        cmdline_len: stage.cmdline_len,
        _cmdline_reserved: 0,
        root_table_phys_addr: final_root_phys,
    };

    unsafe { core::ptr::write_volatile(boot_info_ptr, boot_info) };

    let kernel_stack_top =
        DIRECT_MAP_OFFSET + buffers.kernel_stack_phys + (KERNEL_STACK_PAGES as u64 * PAGE_SIZE) - 8;

    let boot_info_virt = DIRECT_MAP_OFFSET + buffers.bootinfo_phys;

    if paging.la57_transition_root_phys != 0 {
        unsafe {
            enter_kernel_with_la57_fallback(
                paging.la57_transition_root_phys,
                paging.fallback_root_phys,
                kernel_stack_top,
                boot_info_virt,
                KERNEL_PHYS_ADDR,
            );
        }
    } else {
        unsafe {
            asm!(
                "cli",
                "mov cr3, {pml4}",
                "mov rsp, {stack}",
                "mov rdi, {boot_info}",
                "jmp {entry}",
                pml4 = in(reg) paging.root_phys_addr,
                stack = in(reg) kernel_stack_top,
                boot_info = in(reg) boot_info_virt,
                entry = in(reg) KERNEL_PHYS_ADDR,
                options(noreturn)
            );
        }
    }
}
