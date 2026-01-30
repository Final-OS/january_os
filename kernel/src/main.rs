//! january_os 内核 (x86_64)
//!
//! 这是内核的入口点，从 UEFI 引导程序接收完整的系统信息。

#![no_std]
#![no_main]
#![allow(unsafe_op_in_unsafe_fn)]

use core::arch::asm;
use core::panic::PanicInfo;

// ============================================================================
// 与引导程序共享的结构体定义
// ============================================================================

/// 帧缓冲区信息
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

/// 内存区域描述符
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryRegion {
    pub phys_start: u64,
    pub virt_start: u64,
    pub page_count: u64,
    pub region_type: u32,
    pub attributes: u32,
}

/// 内存区域类型
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryRegionType {
    Usable = 0,
    Reserved = 1,
    AcpiReclaimable = 2,
    AcpiNvs = 3,
    Mmio = 4,
    BootloaderReclaimable = 5,
    KernelAndModules = 6,
    Framebuffer = 7,
}

/// 磁盘信息
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

/// 主引导信息结构体
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
    pub kernel_size: u64,

    pub cmdline_addr: u64,
    pub cmdline_len: u32,
    pub _cmdline_reserved: u32,
}

/// BootInfo 魔数
const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000;

// ============================================================================
// 串口驱动
// ============================================================================

const COM1: u16 = 0x3F8;

unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
    value
}

fn serial_init() {
    unsafe {
        outb(COM1 + 1, 0x00);  // 禁用中断
        outb(COM1 + 3, 0x80);  // 启用 DLAB
        outb(COM1 + 0, 0x03);  // 波特率 38400
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);  // 8N1
        outb(COM1 + 2, 0xC7);  // 启用 FIFO
        outb(COM1 + 4, 0x0B);  // IRQ 启用, RTS/DSR 设置
    }
}

fn serial_write_char(c: u8) {
    unsafe {
        while (inb(COM1 + 5) & 0x20) == 0 {}
        outb(COM1, c);
    }
}

fn serial_write(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            serial_write_char(b'\r');
        }
        serial_write_char(b);
    }
}

fn serial_write_hex(val: u64) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    serial_write("0x");
    
    if val == 0 {
        serial_write_char(b'0');
        return;
    }
    
    let mut started = false;
    for i in (0..16).rev() {
        let digit = ((val >> (i * 4)) & 0xF) as usize;
        if digit != 0 || started {
            serial_write_char(HEX[digit]);
            started = true;
        }
    }
}

fn serial_write_dec(val: u64) {
    if val == 0 {
        serial_write_char(b'0');
        return;
    }
    
    let mut buf = [0u8; 20];
    let mut i = 0;
    let mut v = val;
    
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    
    while i > 0 {
        i -= 1;
        serial_write_char(buf[i]);
    }
}

fn serial_write_size(bytes: u64) {
    if bytes >= 1024 * 1024 * 1024 {
        serial_write_dec(bytes / 1024 / 1024 / 1024);
        serial_write(" GB");
    } else if bytes >= 1024 * 1024 {
        serial_write_dec(bytes / 1024 / 1024);
        serial_write(" MB");
    } else if bytes >= 1024 {
        serial_write_dec(bytes / 1024);
        serial_write(" KB");
    } else {
        serial_write_dec(bytes);
        serial_write(" bytes");
    }
}

// ============================================================================
// 帧缓冲区绘制
// ============================================================================

fn fill_rect(fb: &FramebufferInfo, x: u32, y: u32, w: u32, h: u32, color: u32) {
    let fb_ptr = fb.address as *mut u32;
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < fb.width && py < fb.height {
                unsafe {
                    let offset = (py * fb.stride + px) as usize;
                    *fb_ptr.add(offset) = color;
                }
            }
        }
    }
}

fn draw_char(fb: &FramebufferInfo, x: u32, y: u32, c: char, color: u32, scale: u32) {
    // 简单的 5x7 字体
    const FONT: [[u8; 5]; 128] = {
        let mut f = [[0u8; 5]; 128];
        // 空格
        f[b' ' as usize] = [0x00, 0x00, 0x00, 0x00, 0x00];
        // 数字
        f[b'0' as usize] = [0x3E, 0x51, 0x49, 0x45, 0x3E];
        f[b'1' as usize] = [0x00, 0x42, 0x7F, 0x40, 0x00];
        f[b'2' as usize] = [0x42, 0x61, 0x51, 0x49, 0x46];
        f[b'3' as usize] = [0x21, 0x41, 0x45, 0x4B, 0x31];
        f[b'4' as usize] = [0x18, 0x14, 0x12, 0x7F, 0x10];
        f[b'5' as usize] = [0x27, 0x45, 0x45, 0x45, 0x39];
        f[b'6' as usize] = [0x3C, 0x4A, 0x49, 0x49, 0x30];
        f[b'7' as usize] = [0x01, 0x71, 0x09, 0x05, 0x03];
        f[b'8' as usize] = [0x36, 0x49, 0x49, 0x49, 0x36];
        f[b'9' as usize] = [0x06, 0x49, 0x49, 0x29, 0x1E];
        // 大写字母
        f[b'A' as usize] = [0x7E, 0x11, 0x11, 0x11, 0x7E];
        f[b'B' as usize] = [0x7F, 0x49, 0x49, 0x49, 0x36];
        f[b'C' as usize] = [0x3E, 0x41, 0x41, 0x41, 0x22];
        f[b'D' as usize] = [0x7F, 0x41, 0x41, 0x22, 0x1C];
        f[b'E' as usize] = [0x7F, 0x49, 0x49, 0x49, 0x41];
        f[b'F' as usize] = [0x7F, 0x09, 0x09, 0x09, 0x01];
        f[b'G' as usize] = [0x3E, 0x41, 0x49, 0x49, 0x7A];
        f[b'H' as usize] = [0x7F, 0x08, 0x08, 0x08, 0x7F];
        f[b'I' as usize] = [0x00, 0x41, 0x7F, 0x41, 0x00];
        f[b'J' as usize] = [0x20, 0x40, 0x41, 0x3F, 0x01];
        f[b'K' as usize] = [0x7F, 0x08, 0x14, 0x22, 0x41];
        f[b'L' as usize] = [0x7F, 0x40, 0x40, 0x40, 0x40];
        f[b'M' as usize] = [0x7F, 0x02, 0x0C, 0x02, 0x7F];
        f[b'N' as usize] = [0x7F, 0x04, 0x08, 0x10, 0x7F];
        f[b'O' as usize] = [0x3E, 0x41, 0x41, 0x41, 0x3E];
        f[b'P' as usize] = [0x7F, 0x09, 0x09, 0x09, 0x06];
        f[b'Q' as usize] = [0x3E, 0x41, 0x51, 0x21, 0x5E];
        f[b'R' as usize] = [0x7F, 0x09, 0x19, 0x29, 0x46];
        f[b'S' as usize] = [0x46, 0x49, 0x49, 0x49, 0x31];
        f[b'T' as usize] = [0x01, 0x01, 0x7F, 0x01, 0x01];
        f[b'U' as usize] = [0x3F, 0x40, 0x40, 0x40, 0x3F];
        f[b'V' as usize] = [0x1F, 0x20, 0x40, 0x20, 0x1F];
        f[b'W' as usize] = [0x3F, 0x40, 0x38, 0x40, 0x3F];
        f[b'X' as usize] = [0x63, 0x14, 0x08, 0x14, 0x63];
        f[b'Y' as usize] = [0x07, 0x08, 0x70, 0x08, 0x07];
        f[b'Z' as usize] = [0x61, 0x51, 0x49, 0x45, 0x43];
        // 小写字母
        f[b'a' as usize] = [0x20, 0x54, 0x54, 0x54, 0x78];
        f[b'b' as usize] = [0x7F, 0x48, 0x44, 0x44, 0x38];
        f[b'c' as usize] = [0x38, 0x44, 0x44, 0x44, 0x20];
        f[b'd' as usize] = [0x38, 0x44, 0x44, 0x48, 0x7F];
        f[b'e' as usize] = [0x38, 0x54, 0x54, 0x54, 0x18];
        f[b'f' as usize] = [0x08, 0x7E, 0x09, 0x01, 0x02];
        f[b'g' as usize] = [0x0C, 0x52, 0x52, 0x52, 0x3E];
        f[b'h' as usize] = [0x7F, 0x08, 0x04, 0x04, 0x78];
        f[b'i' as usize] = [0x00, 0x44, 0x7D, 0x40, 0x00];
        f[b'j' as usize] = [0x20, 0x40, 0x44, 0x3D, 0x00];
        f[b'k' as usize] = [0x7F, 0x10, 0x28, 0x44, 0x00];
        f[b'l' as usize] = [0x00, 0x41, 0x7F, 0x40, 0x00];
        f[b'm' as usize] = [0x7C, 0x04, 0x18, 0x04, 0x78];
        f[b'n' as usize] = [0x7C, 0x08, 0x04, 0x04, 0x78];
        f[b'o' as usize] = [0x38, 0x44, 0x44, 0x44, 0x38];
        f[b'p' as usize] = [0x7C, 0x14, 0x14, 0x14, 0x08];
        f[b'q' as usize] = [0x08, 0x14, 0x14, 0x18, 0x7C];
        f[b'r' as usize] = [0x7C, 0x08, 0x04, 0x04, 0x08];
        f[b's' as usize] = [0x48, 0x54, 0x54, 0x54, 0x20];
        f[b't' as usize] = [0x04, 0x3F, 0x44, 0x40, 0x20];
        f[b'u' as usize] = [0x3C, 0x40, 0x40, 0x20, 0x7C];
        f[b'v' as usize] = [0x1C, 0x20, 0x40, 0x20, 0x1C];
        f[b'w' as usize] = [0x3C, 0x40, 0x30, 0x40, 0x3C];
        f[b'x' as usize] = [0x44, 0x28, 0x10, 0x28, 0x44];
        f[b'y' as usize] = [0x0C, 0x50, 0x50, 0x50, 0x3C];
        f[b'z' as usize] = [0x44, 0x64, 0x54, 0x4C, 0x44];
        // 符号
        f[b'_' as usize] = [0x40, 0x40, 0x40, 0x40, 0x40];
        f[b'-' as usize] = [0x08, 0x08, 0x08, 0x08, 0x08];
        f[b'.' as usize] = [0x00, 0x60, 0x60, 0x00, 0x00];
        f[b':' as usize] = [0x00, 0x36, 0x36, 0x00, 0x00];
        f[b'/' as usize] = [0x20, 0x10, 0x08, 0x04, 0x02];
        f[b'=' as usize] = [0x14, 0x14, 0x14, 0x14, 0x14];
        f[b'[' as usize] = [0x00, 0x7F, 0x41, 0x41, 0x00];
        f[b']' as usize] = [0x00, 0x41, 0x41, 0x7F, 0x00];
        f[b'(' as usize] = [0x00, 0x1C, 0x22, 0x41, 0x00];
        f[b')' as usize] = [0x00, 0x41, 0x22, 0x1C, 0x00];
        f[b'x' as usize] = [0x44, 0x28, 0x10, 0x28, 0x44];
        f
    };
    
    let idx = (c as usize).min(127);
    let glyph = FONT[idx];
    
    for (col, &bits) in glyph.iter().enumerate() {
        for row in 0..7 {
            if (bits >> row) & 1 != 0 {
                let px = x + (col as u32) * scale;
                let py = y + (row as u32) * scale;
                fill_rect(fb, px, py, scale, scale, color);
            }
        }
    }
}

fn draw_string(fb: &FramebufferInfo, x: u32, y: u32, s: &str, color: u32, scale: u32) {
    let mut cx = x;
    for c in s.chars() {
        draw_char(fb, cx, y, c, color, scale);
        cx += 6 * scale;
    }
}

// ============================================================================
// 内核入口点
// ============================================================================

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
pub unsafe extern "C" fn _start(boot_info_ptr: *const BootInfo) -> ! {
    // 初始化串口
    serial_init();

    serial_write("\n");
    serial_write("================================================================\n");
    serial_write("              january_os Kernel v0.1.0\n");
    serial_write("================================================================\n");
    serial_write("\n");

    // 验证 BootInfo
    if boot_info_ptr.is_null() {
        serial_write("FATAL: BootInfo pointer is NULL!\n");
        halt();
    }

    let info = &*boot_info_ptr;

    if info.magic != BOOTINFO_MAGIC {
        serial_write("FATAL: Invalid BootInfo magic number!\n");
        serial_write("  Expected: ");
        serial_write_hex(BOOTINFO_MAGIC);
        serial_write("\n  Got:      ");
        serial_write_hex(info.magic);
        serial_write("\n");
        halt();
    }

    serial_write("BootInfo validated successfully.\n");
    serial_write("  Version: ");
    serial_write_dec(info.version as u64);
    serial_write("\n  Size: ");
    serial_write_dec(info.size as u64);
    serial_write(" bytes\n");
    serial_write("\n");

    // ========== 帧缓冲区信息 ==========
    serial_write("=== FRAMEBUFFER ===\n");
    serial_write("  Address:        ");
    serial_write_hex(info.framebuffer.address);
    serial_write("\n");
    serial_write("  Size:           ");
    serial_write_size(info.framebuffer.size);
    serial_write("\n");
    serial_write("  Resolution:     ");
    serial_write_dec(info.framebuffer.width as u64);
    serial_write(" x ");
    serial_write_dec(info.framebuffer.height as u64);
    serial_write("\n");
    serial_write("  Stride:         ");
    serial_write_dec(info.framebuffer.stride as u64);
    serial_write(" pixels/line\n");
    serial_write("  Bytes/Pixel:    ");
    serial_write_dec(info.framebuffer.bytes_per_pixel as u64);
    serial_write("\n");
    serial_write("  Pixel Format:   ");
    match info.framebuffer.pixel_format {
        0 => serial_write("RGB"),
        1 => serial_write("BGR"),
        2 => serial_write("Bitmask"),
        3 => serial_write("BltOnly"),
        _ => serial_write("Unknown"),
    }
    serial_write("\n\n");

    // ========== 内存信息 ==========
    serial_write("=== MEMORY ===\n");
    serial_write("  Total Memory:   ");
    serial_write_size(info.total_memory);
    serial_write("\n");
    serial_write("  Usable Memory:  ");
    serial_write_size(info.usable_memory);
    serial_write("\n");
    serial_write("  Memory Map:     ");
    serial_write_dec(info.memory_map_entries as u64);
    serial_write(" entries at ");
    serial_write_hex(info.memory_map_addr);
    serial_write("\n");
    serial_write("  Entry Size:     ");
    serial_write_dec(info.memory_map_entry_size as u64);
    serial_write(" bytes\n\n");

    // 打印内存映射详情
    serial_write("  Memory Map Details:\n");
    serial_write("  ---------------------------------------------------------\n");
    serial_write("  #    Start Address     Pages       Size       Type\n");
    serial_write("  ---------------------------------------------------------\n");
    
    let mem_regions = info.memory_map_addr as *const MemoryRegion;
    let mut usable_regions = 0u32;
    for i in 0..info.memory_map_entries.min(20) {  // 只打印前20个
        let region = &*mem_regions.add(i as usize);
        
        // 序号
        serial_write("  ");
        if i < 10 { serial_write(" "); }
        serial_write_dec(i as u64);
        serial_write("   ");
        
        // 地址
        serial_write_hex(region.phys_start);
        serial_write("  ");
        
        // 页数
        let pages = region.page_count;
        let mut pad_str = "         ";
        if pages >= 10 { pad_str = "        "; }
        if pages >= 100 { pad_str = "       "; }
        if pages >= 1000 { pad_str = "      "; }
        if pages >= 10000 { pad_str = "     "; }
        if pages >= 100000 { pad_str = "    "; }
        serial_write_dec(pages);
        serial_write(pad_str);
        
        // 大小
        let size = pages * 4096;
        if size >= 1024 * 1024 {
            serial_write_dec(size / 1024 / 1024);
            serial_write(" MB     ");
        } else if size >= 1024 {
            serial_write_dec(size / 1024);
            serial_write(" KB     ");
        } else {
            serial_write_dec(size);
            serial_write(" B      ");
        }
        
        // 类型
        match region.region_type {
            0 => { serial_write("Usable"); usable_regions += 1; }
            1 => serial_write("Reserved"),
            2 => serial_write("ACPI Reclaimable"),
            3 => serial_write("ACPI NVS"),
            4 => serial_write("MMIO"),
            5 => serial_write("Bootloader"),
            6 => serial_write("Kernel"),
            7 => serial_write("Framebuffer"),
            _ => serial_write("Unknown"),
        }
        serial_write("\n");
    }
    
    if info.memory_map_entries > 20 {
        serial_write("  ... (");
        serial_write_dec((info.memory_map_entries - 20) as u64);
        serial_write(" more entries)\n");
    }
    serial_write("  ---------------------------------------------------------\n");
    serial_write("  Usable regions: ");
    serial_write_dec(usable_regions as u64);
    serial_write("\n\n");

    // ========== ACPI 信息 ==========
    serial_write("=== ACPI ===\n");
    if info.acpi_rsdp_addr != 0 {
        serial_write("  RSDP Address:   ");
        serial_write_hex(info.acpi_rsdp_addr);
        serial_write("\n");
        serial_write("  ACPI Version:   ");
        serial_write_dec(info.acpi_version as u64);
        serial_write(".0\n");
        
        // 尝试读取 RSDP 签名
        let rsdp = info.acpi_rsdp_addr as *const u8;
        serial_write("  RSDP Signature: ");
        for i in 0..8 {
            let c = *rsdp.add(i);
            if c >= 0x20 && c < 0x7F {
                serial_write_char(c);
            }
        }
        serial_write("\n");
    } else {
        serial_write("  Not available\n");
    }
    serial_write("\n");

    // ========== SMBIOS 信息 ==========
    serial_write("=== SMBIOS ===\n");
    if info.smbios_addr != 0 {
        serial_write("  Entry Point:    ");
        serial_write_hex(info.smbios_addr);
        serial_write("\n");
        serial_write("  SMBIOS Version: ");
        serial_write_dec(info.smbios_version as u64);
        serial_write(".x\n");
    } else {
        serial_write("  Not available\n");
    }
    serial_write("\n");

    // ========== 存储设备信息 ==========
    serial_write("=== STORAGE DEVICES ===\n");
    serial_write("  Disk Count:     ");
    serial_write_dec(info.disk_count as u64);
    serial_write("\n");
    serial_write("  Boot Disk:      ");
    if info.boot_disk_index >= 0 {
        serial_write("#");
        serial_write_dec(info.boot_disk_index as u64);
    } else {
        serial_write("Unknown");
    }
    serial_write("\n\n");

    if info.disk_count > 0 {
        serial_write("  Disk Details:\n");
        serial_write("  -----------------------------------------------------\n");
        serial_write("  #  Type      Removable  Size         Block Size\n");
        serial_write("  -----------------------------------------------------\n");
        
        let disks = info.disk_info_addr as *const DiskInfo;
        for i in 0..info.disk_count.min(16) {
            let disk = &*disks.add(i as usize);
            
            serial_write("  ");
            serial_write_dec(i as u64);
            serial_write("  ");
            
            // 类型
            match disk.disk_type {
                0 => serial_write("Unknown   "),
                1 => serial_write("HDD       "),
                2 => serial_write("CD-ROM    "),
                3 => serial_write("USB       "),
                4 => serial_write("NVMe      "),
                5 => serial_write("Floppy    "),
                _ => serial_write("Other     "),
            }
            
            // 可移动
            if disk.removable != 0 {
                serial_write("Yes        ");
            } else {
                serial_write("No         ");
            }
            
            // 大小
            let size = disk.total_size;
            if size >= 1024 * 1024 * 1024 {
                serial_write_dec(size / 1024 / 1024 / 1024);
                serial_write(" GB        ");
            } else if size >= 1024 * 1024 {
                serial_write_dec(size / 1024 / 1024);
                serial_write(" MB        ");
            } else {
                serial_write_dec(size / 1024);
                serial_write(" KB        ");
            }
            
            // 块大小
            serial_write_dec(disk.block_size);
            serial_write(" bytes");
            
            if disk.boot_device != 0 {
                serial_write(" [BOOT]");
            }
            serial_write("\n");
        }
        serial_write("  -----------------------------------------------------\n");
    }
    serial_write("\n");

    // ========== UEFI 运行时服务 ==========
    serial_write("=== UEFI RUNTIME SERVICES ===\n");
    serial_write("  Address:        ");
    serial_write_hex(info.uefi_runtime_services);
    serial_write("\n\n");

    // ========== 内核信息 ==========
    serial_write("=== KERNEL ===\n");
    serial_write("  Load Address:   ");
    serial_write_hex(info.kernel_phys_addr);
    serial_write("\n");
    serial_write("  Size:           ");
    serial_write_size(info.kernel_size);
    serial_write("\n\n");

    // ========== 命令行 ==========
    serial_write("=== COMMAND LINE ===\n");
    if info.cmdline_addr != 0 && info.cmdline_len > 0 {
        serial_write("  \"");
        let cmdline = info.cmdline_addr as *const u8;
        for i in 0..info.cmdline_len.min(256) {
            let c = *cmdline.add(i as usize);
            if c == 0 { break; }
            serial_write_char(c);
        }
        serial_write("\"\n");
    } else {
        serial_write("  (none)\n");
    }
    serial_write("\n");

    serial_write("================================================================\n");
    serial_write("                   Boot Information Complete\n");
    serial_write("================================================================\n");
    serial_write("\n");

    // ========== 图形测试 ==========
    serial_write("Drawing to framebuffer...\n");
    
    let fb = &info.framebuffer;
    
    // 背景色 (深蓝色)
    let bg_color = 0x001a1a2e;
    // 填充背景
    for y in 0..fb.height {
        for x in 0..fb.width {
            let offset = (y * fb.stride + x) as usize;
            *((fb.address as *mut u32).add(offset)) = bg_color;
        }
    }

    // 标题
    let title_y = 50;
    draw_string(fb, 50, title_y, "january_os", 0x00FFFFFF, 4);
    
    // 副标题
    draw_string(fb, 50, title_y + 40, "Kernel loaded successfully", 0x0088FF88, 2);
    
    // 系统信息
    let info_y = title_y + 100;
    let info_color = 0x00AAAAAA;
    
    draw_string(fb, 50, info_y, "System Information:", 0x00FFFF00, 2);
    
    // 分辨率
    draw_string(fb, 50, info_y + 30, "Resolution:", info_color, 1);
    
    // 内存
    draw_string(fb, 50, info_y + 50, "Memory:", info_color, 1);
    
    // ACPI
    draw_string(fb, 50, info_y + 70, "ACPI:", info_color, 1);
    if info.acpi_rsdp_addr != 0 {
        draw_string(fb, 150, info_y + 70, "Available", 0x0088FF88, 1);
    } else {
        draw_string(fb, 150, info_y + 70, "Not found", 0x00FF8888, 1);
    }
    
    // 磁盘数量
    draw_string(fb, 50, info_y + 90, "Disks:", info_color, 1);
    
    // 状态指示器
    let status_y = fb.height - 50;
    fill_rect(fb, 50, status_y, 20, 20, 0x0000FF00);  // 绿色方块
    draw_string(fb, 80, status_y + 5, "Kernel running", 0x00FFFFFF, 1);

    serial_write("Framebuffer updated!\n");
    serial_write("\n");
    serial_write("Kernel initialization complete. Halting.\n");

    halt();
}

fn halt() -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_write("\n!!! KERNEL PANIC !!!\n");
    if let Some(location) = info.location() {
        serial_write("Location: ");
        serial_write(location.file());
        serial_write(":");
        serial_write_dec(location.line() as u64);
        serial_write("\n");
    }
    halt();
}
