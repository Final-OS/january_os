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
#[derive(Clone, Copy, Default)]
pub struct KernelVaLayout {
    pub va_bits: u8,
    pub page_levels: u8,
    pub _reserved0: [u8; 6],
    pub direct_map_start: u64,
    pub direct_map_end: u64,
    pub vmalloc_start: u64,
    pub vmalloc_end: u64,
    pub vmemmap_start: u64,
    pub vmemmap_end: u64,
    pub modules_start: u64,
    pub modules_end: u64,
    pub fixmap_start: u64,
    pub fixmap_end: u64,
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
    pub kernel_layout: KernelVaLayout,
    pub cmdline_addr: u64,
    pub cmdline_len: u32,
    pub _cmdline_reserved: u32,
    pub initramfs_phys_addr: u64,
    pub initramfs_size: u64,
    pub root_table_phys_addr: u64,
}

pub const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000;
/// 与 bootloader `boot/x86_64/src/bootinfo.rs::MAX_MEMORY_REGIONS` 保持一致。
pub const MAX_MEMORY_REGIONS: usize = 256;
pub const DEFAULT_INITRD_COMMAND: &str = "/bin/init";

impl BootInfo {
    #[inline]
    pub fn page_table_root_phys(&self) -> u64 {
        if self.version >= 4 && self.root_table_phys_addr != 0 {
            self.root_table_phys_addr
        } else {
            self.pml4_phys_addr
        }
    }

    #[inline]
    pub fn cmdline(&self) -> &str {
        if self.cmdline_addr == 0 || self.cmdline_len == 0 {
            return "";
        }

        let ptr = self.cmdline_addr as *const u8;
        let len = self.cmdline_len as usize;
        let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
        core::str::from_utf8(bytes).unwrap_or("")
    }

    #[inline]
    pub fn initrd_command(&self) -> &str {
        cmdline_value(self.cmdline(), "initrd")
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_INITRD_COMMAND)
    }
}

#[inline]
fn cmdline_value<'a>(cmdline: &'a str, key: &str) -> Option<&'a str> {
    for token in cmdline.split_ascii_whitespace() {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if name == key {
            return Some(value);
        }
    }
    None
}
