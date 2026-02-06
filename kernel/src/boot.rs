//! 引导信息定义
//!
//! 包含 Bootloader 传递给内核的结构体定义。

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub address: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bytes_per_pixel: u32,
    pub pixel_format: u32,
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DiskInfo {
    pub disk_type: u32,
    pub removable: u32,
    pub boot_device: u32,
    pub read_only: u32,
    pub block_size: u64,
    pub total_blocks: u64,
    pub total_size: u64,
    pub media_id: u32,
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub size: u32,
    pub framebuffer: FramebufferInfo,
    pub memory_map_addr: u64,
    pub memory_map_entries: u32,
    pub memory_map_entry_size: u32,
    pub total_memory: u64,
    pub usable_memory: u64,
    pub acpi_rsdp_addr: u64,
    pub acpi_version: u32,
    pub _acpi_reserved: u32,
    pub smbios_addr: u64,
    pub smbios_version: u32,
    pub _smbios_reserved: u32,
    pub disk_info_addr: u64,
    pub disk_count: u32,
    pub boot_disk_index: i32,
    pub uefi_runtime_services: u64,
    pub kernel_phys_addr: u64,
    pub kernel_virt_addr: u64,
    pub kernel_size: u64,
    pub pml4_phys_addr: u64,
    pub direct_map_offset: u64,
    pub cmdline_addr: u64,
    pub cmdline_len: u32,
    pub _cmdline_reserved: u32,
}

pub const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000;
