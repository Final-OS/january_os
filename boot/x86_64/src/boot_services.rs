//! UEFI 引导阶段设备与配置表查询

use uefi::boot::{self, MemoryType};
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::media::block::BlockIO;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::Identify;

use crate::bootinfo::{
    DiskInfo, DiskType, FramebufferInfo, PixelFormatType, KERNEL_PHYS_ADDR, MAX_DISKS,
};

const KERNEL_PATH: &uefi::CStr16 = uefi::cstr16!("\\EFI\\january_os\\kernel.bin");
const INITRAMFS_PATH: &uefi::CStr16 = uefi::cstr16!("\\EFI\\january_os\\initramfs.cpio");

fn load_file_to_pages(
    path: &uefi::CStr16,
    fixed_phys: Option<u64>,
    memory_type: MemoryType,
) -> Option<(u64, usize)> {
    let fs_handle = boot::get_handle_for_protocol::<SimpleFileSystem>().ok()?;
    let mut fs = boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle).ok()?;
    let mut root = fs.open_volume().ok()?;

    let file_handle = root
        .open(path, FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut file = file_handle.into_regular_file()?;

    let mut info_buf = [0u8; 512];
    let file_info: &FileInfo = file.get_info(&mut info_buf).ok()?;
    let file_size = file_info.file_size() as usize;
    let pages = (file_size + 4095) / 4096;
    if pages == 0 {
        return None;
    }

    let alloc_type = match fixed_phys {
        Some(phys) => boot::AllocateType::Address(phys),
        None => boot::AllocateType::AnyPages,
    };
    let phys = boot::allocate_pages(alloc_type, memory_type, pages)
        .ok()?
        .as_ptr() as u64;
    let buffer = unsafe { core::slice::from_raw_parts_mut(phys as *mut u8, file_size) };
    file.read(buffer).ok()?;

    Some((phys, file_size))
}
pub fn setup_graphics() -> FramebufferInfo {
    let gop_handle = match boot::get_handle_for_protocol::<GraphicsOutput>() {
        Ok(h) => h,
        Err(_) => {
            return FramebufferInfo {
                address: 0,
                size: 0,
                width: 0,
                height: 0,
                stride: 0,
                bytes_per_pixel: 4,
                pixel_format: PixelFormatType::Bgr as u32,
                _reserved: 0,
            };
        }
    };

    let gop = unsafe {
        boot::open_protocol::<GraphicsOutput>(
            boot::OpenProtocolParams {
                handle: gop_handle,
                agent: boot::image_handle(),
                controller: None,
            },
            boot::OpenProtocolAttributes::GetProtocol,
        )
    };

    let mut gop = match gop {
        Ok(g) => g,
        Err(_) => {
            return FramebufferInfo {
                address: 0,
                size: 0,
                width: 0,
                height: 0,
                stride: 0,
                bytes_per_pixel: 4,
                pixel_format: PixelFormatType::Bgr as u32,
                _reserved: 0,
            };
        }
    };

    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();
    let stride = mode_info.stride() as u32;

    let mut fb = gop.frame_buffer();
    let fb_addr = fb.as_mut_ptr() as u64;
    let fb_size = fb.size() as u64;

    let pixel_format = match mode_info.pixel_format() {
        PixelFormat::Rgb => PixelFormatType::Rgb as u32,
        PixelFormat::Bgr => PixelFormatType::Bgr as u32,
        PixelFormat::Bitmask => PixelFormatType::Bitmask as u32,
        PixelFormat::BltOnly => PixelFormatType::BltOnly as u32,
    };

    FramebufferInfo {
        address: fb_addr,
        size: fb_size,
        width: width as u32,
        height: height as u32,
        stride,
        bytes_per_pixel: 4,
        pixel_format,
        _reserved: 0,
    }
}

pub fn load_kernel() -> usize {
    let (phys, size) =
        load_file_to_pages(KERNEL_PATH, Some(KERNEL_PHYS_ADDR), MemoryType::LOADER_CODE)
            .expect("Failed to load kernel file");
    assert_eq!(phys, KERNEL_PHYS_ADDR, "kernel physical address mismatch");
    size
}

pub fn load_initramfs() -> (u64, u64) {
    let Some((phys, size)) = load_file_to_pages(INITRAMFS_PATH, None, MemoryType::LOADER_DATA)
    else {
        return (0, 0);
    };
    (phys, size as u64)
}

pub fn scan_disks(diskinfo_phys: u64) -> (u32, i32) {
    let disk_info_base = diskinfo_phys as *mut DiskInfo;
    let mut count = 0u32;
    let mut boot_disk = -1i32;

    let handles = match boot::locate_handle_buffer(boot::SearchType::ByProtocol(&BlockIO::GUID)) {
        Ok(h) => h,
        Err(_) => return (0, -1),
    };

    for handle in handles.iter() {
        if count >= MAX_DISKS as u32 {
            break;
        }

        if let Ok(block_io) = boot::open_protocol_exclusive::<BlockIO>(*handle) {
            let media = block_io.media();

            if !media.is_media_present() {
                continue;
            }

            let disk_type = if media.is_removable_media() {
                if media.block_size() == 2048 {
                    DiskType::CdRom as u32
                } else {
                    DiskType::Usb as u32
                }
            } else {
                DiskType::HardDisk as u32
            };

            let total_blocks = media.last_block() + 1;
            let block_size = media.block_size() as u64;
            let total_size = total_blocks * block_size;

            let disk_info = DiskInfo {
                disk_type,
                removable: if media.is_removable_media() { 1 } else { 0 },
                boot_device: 0,
                read_only: if media.is_read_only() { 1 } else { 0 },
                block_size,
                total_blocks,
                total_size,
                media_id: media.media_id(),
                _reserved: 0,
            };

            unsafe {
                core::ptr::write_volatile(disk_info_base.add(count as usize), disk_info);
            }

            count += 1;
        }
    }

    unsafe {
        for i in 0..count {
            let disk = &*disk_info_base.add(i as usize);
            if disk.removable == 0 && disk.disk_type == DiskType::HardDisk as u32 {
                boot_disk = i as i32;
                (*disk_info_base.add(i as usize)).boot_device = 1;
                break;
            }
        }
    }

    (count, boot_disk)
}

pub fn find_acpi_rsdp() -> (u64, u32) {
    const ACPI2_GUID: uefi::Guid = uefi::guid!("8868e871-e4f1-11d3-bc22-0080c73c8881");
    const ACPI1_GUID: uefi::Guid = uefi::guid!("eb9d2d30-2d88-11d3-9a16-0090273fc14d");

    uefi::system::with_config_table(|tables| {
        for table in tables {
            if table.guid == ACPI2_GUID {
                return (table.address as u64, 2);
            }
        }
        for table in tables {
            if table.guid == ACPI1_GUID {
                return (table.address as u64, 1);
            }
        }
        (0, 0)
    })
}

pub fn find_smbios() -> (u64, u32) {
    const SMBIOS3_GUID: uefi::Guid = uefi::guid!("f2fd1544-9794-4a2c-992e-e5bbcf20e394");
    const SMBIOS_GUID: uefi::Guid = uefi::guid!("eb9d2d31-2d88-11d3-9a16-0090273fc14d");

    uefi::system::with_config_table(|tables| {
        let mut smbios3: Option<u64> = None;
        let mut smbios2: Option<u64> = None;

        for table in tables {
            if table.guid == SMBIOS3_GUID {
                smbios3 = Some(table.address as u64);
            } else if table.guid == SMBIOS_GUID {
                smbios2 = Some(table.address as u64);
            }
        }

        if let Some(addr) = smbios3 {
            return (addr, 3);
        }

        if let Some(addr) = smbios2 {
            return (addr, 2);
        }

        (0, 0)
    })
}

pub fn get_runtime_services() -> u64 {
    let Some(st) = uefi::table::system_table_raw() else {
        return 0;
    };

    let st = unsafe { st.as_ref() };
    if st.runtime_services.is_null() {
        0
    } else {
        st.runtime_services as u64
    }
}
