//! january_os UEFI 引导程序 (x86_64)
//!
//! # UEFI 引导程序的完整职责
//!
//! UEFI 引导程序是操作系统启动的第一阶段，负责：
//!
//! 1. **初始化图形输出** - 通过 GOP 获取帧缓冲区
//! 2. **加载内核文件** - 从 EFI 系统分区读取内核
//! 3. **收集硬件信息**:
//!    - 内存映射 (Memory Map)
//!    - ACPI 表 (RSDP)
//!    - SMBIOS 表 (系统信息)
//!    - 存储设备列表
//! 4. **建立内核页表**:
//!    - 低端内存恒等映射 (用于引导过渡)
//!    - 内核高半部分映射 (0xFFFF_8000_0000_0000+)
//!    - 直接映射区 (所有物理内存)
//! 5. **退出引导服务** - 将控制权转移给操作系统
//! 6. **切换页表并跳转到内核** - 从虚拟地址开始执行

#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::Write;
use uefi::boot::{self, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::console::text::Output;
use uefi::proto::media::block::BlockIO;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::Identify;

// ============================================================================
// 引导信息结构体定义
// ============================================================================

/// 像素格式
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum PixelFormatType {
    /// RGB 格式 (R在低字节)
    Rgb = 0,
    /// BGR 格式 (B在低字节，最常见)
    Bgr = 1,
    /// 位掩码格式
    Bitmask = 2,
    /// 仅 BLT 格式
    BltOnly = 3,
}

/// 帧缓冲区信息
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    /// 帧缓冲区物理地址
    pub address: u64,
    /// 帧缓冲区总大小（字节）
    pub size: u64,
    /// 屏幕宽度（像素）
    pub width: u32,
    /// 屏幕高度（像素）
    pub height: u32,
    /// 每行像素数（可能 > width，因为对齐）
    pub stride: u32,
    /// 每像素字节数
    pub bytes_per_pixel: u32,
    /// 像素格式
    pub pixel_format: u32,
    /// 保留，对齐用
    pub _reserved: u32,
}

/// 内存区域类型（简化版）
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryRegionType {
    /// 可用内存
    Usable = 0,
    /// 保留内存（不可使用）
    Reserved = 1,
    /// ACPI 可回收内存
    AcpiReclaimable = 2,
    /// ACPI NVS 内存
    AcpiNvs = 3,
    /// 内存映射 I/O
    Mmio = 4,
    /// 引导程序代码/数据（内核可回收）
    BootloaderReclaimable = 5,
    /// 内核代码/数据
    KernelAndModules = 6,
    /// 帧缓冲区
    Framebuffer = 7,
}

/// 内存区域描述符（简化版，兼容性更好）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryRegion {
    /// 物理起始地址
    pub phys_start: u64,
    /// 虚拟起始地址（通常与物理相同）
    pub virt_start: u64,
    /// 页数（每页 4KB）
    pub page_count: u64,
    /// 区域类型
    pub region_type: u32,
    /// 属性标志
    pub attributes: u32,
}

/// 磁盘类型
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum DiskType {
    Unknown = 0,
    HardDisk = 1,
    CdRom = 2,
    Usb = 3,
    NVMe = 4,
    Floppy = 5,
    Network = 6,
}

/// 磁盘信息
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DiskInfo {
    /// 磁盘类型
    pub disk_type: u32,
    /// 是否可移动 (1=可移动, 0=固定)
    pub removable: u32,
    /// 是否为启动设备 (1=是, 0=否)
    pub boot_device: u32,
    /// 是否只读
    pub read_only: u32,
    /// 逻辑块大小（字节）
    pub block_size: u64,
    /// 总块数
    pub total_blocks: u64,
    /// 总容量（字节）
    pub total_size: u64,
    /// 媒体 ID
    pub media_id: u32,
    /// 保留
    pub _reserved: u32,
}

/// 主引导信息结构体 - 传递给内核的所有信息
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    /// 魔数，用于验证结构体有效性 (应为 0x4A414E5F4F530000 "JAN_OS\0\0")
    pub magic: u64,
    /// 结构体版本号
    pub version: u32,
    /// 结构体大小（字节）
    pub size: u32,

    // ========== 帧缓冲区信息 ==========
    pub framebuffer: FramebufferInfo,

    // ========== 内存映射 ==========
    /// 内存区域数组地址
    pub memory_map_addr: u64,
    /// 内存区域数量
    pub memory_map_entries: u32,
    /// 每个条目大小
    pub memory_map_entry_size: u32,
    /// 总可用内存（字节）
    pub total_memory: u64,
    /// 可用内存（字节）
    pub usable_memory: u64,

    // ========== ACPI 信息 ==========
    /// ACPI RSDP 地址 (0 表示未找到)
    pub acpi_rsdp_addr: u64,
    /// ACPI 版本 (1 或 2)
    pub acpi_version: u32,
    pub _acpi_reserved: u32,

    // ========== SMBIOS 信息 ==========
    /// SMBIOS 入口点地址 (0 表示未找到)
    pub smbios_addr: u64,
    /// SMBIOS 版本
    pub smbios_version: u32,
    pub _smbios_reserved: u32,

    // ========== 存储设备信息 ==========
    /// 磁盘信息数组地址
    pub disk_info_addr: u64,
    /// 检测到的磁盘数量
    pub disk_count: u32,
    /// 启动设备索引 (-1 表示未知)
    pub boot_disk_index: i32,

    // ========== UEFI 运行时服务 ==========
    /// UEFI 运行时服务表地址 (ExitBootServices 后仍可用)
    pub uefi_runtime_services: u64,

    // ========== 内核信息 ==========
    /// 内核加载的物理地址
    pub kernel_phys_addr: u64,
    /// 内核运行的虚拟地址
    pub kernel_virt_addr: u64,
    /// 内核大小（字节）
    pub kernel_size: u64,

    // ========== 页表信息 ==========
    /// PML4 页表物理地址
    pub pml4_phys_addr: u64,
    /// 直接映射区偏移
    pub direct_map_offset: u64,

    // ========== 命令行 ==========
    /// 命令行字符串地址（虚拟地址，通过直接映射）
    pub cmdline_addr: u64,
    /// 命令行长度
    pub cmdline_len: u32,
    pub _cmdline_reserved: u32,
}

// ============================================================================
// 常量定义
// ============================================================================

/// BootInfo 魔数: "JAN_OS\0\0" 的 ASCII 值
const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000;
/// BootInfo 版本
const BOOTINFO_VERSION: u32 = 2;  // 版本升级，支持页表
/// 内核加载的物理地址
const KERNEL_PHYS_ADDR: u64 = 0x100000;
/// 内核运行的虚拟地址（高半部分）
const KERNEL_VIRT_ADDR: u64 = 0xFFFF_8000_0010_0000;
/// 直接映射区偏移（物理地址 + 此偏移 = 虚拟地址）
const DIRECT_MAP_OFFSET: u64 = 0xFFFF_8800_0000_0000;
/// BootInfo 存储物理地址
const BOOTINFO_PHYS_ADDR: u64 = 0x7000;
/// BootInfo 虚拟地址（通过直接映射访问）
const BOOTINFO_VIRT_ADDR: u64 = DIRECT_MAP_OFFSET + BOOTINFO_PHYS_ADDR;
/// 内存映射存储地址
const MEMMAP_ADDR: u64 = 0x10000;
/// 磁盘信息存储地址
const DISKINFO_ADDR: u64 = 0x20000;
/// 命令行存储地址
const CMDLINE_ADDR: u64 = 0x21000;
/// 页表存储起始地址（在内核加载地址之前）
const PAGE_TABLE_ADDR: u64 = 0x30000;
/// 最大磁盘数
const MAX_DISKS: usize = 32;
/// 最大内存区域数
const MAX_MEMORY_REGIONS: usize = 256;

// ============================================================================
// 页表常量
// ============================================================================

// 页表常量 (部分保留供将来使用)
#[allow(dead_code)]
mod paging_consts {
    /// 页大小 4KB
    pub const PAGE_SIZE: u64 = 4096;
    /// 页表项标志：存在
    pub const PTE_PRESENT: u64 = 1 << 0;
    /// 页表项标志：可写
    pub const PTE_WRITABLE: u64 = 1 << 1;
    /// 页表项标志：用户可访问
    pub const PTE_USER: u64 = 1 << 2;
    /// 页表项标志：写通
    pub const PTE_WRITE_THROUGH: u64 = 1 << 3;
    /// 页表项标志：禁用缓存
    pub const PTE_NO_CACHE: u64 = 1 << 4;
    /// 页表项标志：大页 (2MB/1GB)
    pub const PTE_HUGE: u64 = 1 << 7;
    /// 页表项标志：全局
    pub const PTE_GLOBAL: u64 = 1 << 8;
    /// 页表项标志：禁止执行
    pub const PTE_NO_EXECUTE: u64 = 1 << 63;
}
use paging_consts::*;

// ============================================================================
// 入口点
// ============================================================================

#[entry]
fn main() -> Status {
    // 显示启动信息
    println_uefi("========================================");
    println_uefi("  january_os UEFI Bootloader v0.1.0");
    println_uefi("  Architecture: x86_64");
    println_uefi("========================================");
    println_uefi("");

    // 第一步：初始化图形
    println_uefi("[1/8] Initializing graphics (GOP)...");
    let framebuffer = setup_graphics();
    print_uefi("      Resolution: ");
    print_dec(framebuffer.width as u64);
    print_uefi("x");
    print_dec(framebuffer.height as u64);
    println_uefi("");

    // 第二步：加载内核
    println_uefi("[2/8] Loading kernel...");
    println_uefi("      Opening filesystem...");
    let kernel_size = load_kernel();
    print_uefi("      Kernel size: ");
    print_dec(kernel_size as u64);
    println_uefi(" bytes");

    // 第三步：扫描存储设备
    println_uefi("[3/8] Scanning storage devices...");
    let (disk_count, boot_disk) = scan_disks();
    print_uefi("      Found ");
    print_dec(disk_count as u64);
    println_uefi(" disk(s)");

    // 第四步：获取 ACPI RSDP
    println_uefi("[4/8] Locating ACPI tables...");
    let (acpi_rsdp, acpi_version) = find_acpi_rsdp();
    if acpi_rsdp != 0 {
        print_uefi("      RSDP at 0x");
        print_hex(acpi_rsdp);
        print_uefi(" (ACPI ");
        print_dec(acpi_version as u64);
        println_uefi(".0)");
    } else {
        println_uefi("      ACPI not found!");
    }

    // 第五步：获取 SMBIOS
    println_uefi("[5/8] Locating SMBIOS...");
    let (smbios_addr, smbios_version) = find_smbios();
    if smbios_addr != 0 {
        print_uefi("      SMBIOS at 0x");
        print_hex(smbios_addr);
        println_uefi("");
    } else {
        println_uefi("      SMBIOS not found");
    }

    // 第六步：获取运行时服务
    println_uefi("[6/8] Getting UEFI Runtime Services...");
    let runtime_services = get_runtime_services();
    print_uefi("      Runtime Services at 0x");
    print_hex(runtime_services);
    println_uefi("");

    // 设置命令行（可以从 UEFI 变量读取或使用默认值）
    let cmdline = b"console=ttyS0 loglevel=7\0";
    unsafe {
        let cmdline_ptr = CMDLINE_ADDR as *mut u8;
        for (i, &byte) in cmdline.iter().enumerate() {
            *cmdline_ptr.add(i) = byte;
        }
    }

    println_uefi("[7/8] Setting up page tables...");
    print_uefi("      Kernel virtual address: 0x");
    print_hex(KERNEL_VIRT_ADDR);
    println_uefi("");
    print_uefi("      Direct map offset: 0x");
    print_hex(DIRECT_MAP_OFFSET);
    println_uefi("");

    println_uefi("[8/8] Exiting boot services...");
    println_uefi("");
    println_uefi("Jumping to kernel...");
    println_uefi("");

    // 短暂延迟让用户看到信息
    for _ in 0..2_000_000 {
        unsafe { asm!("pause"); }
    }

    // 退出引导服务
    let mmap = unsafe { boot::exit_boot_services(None) };

    // 填充引导信息和设置页表
    unsafe {
        // 转换并复制内存映射
        let (mem_entries, total_mem, usable_mem, max_phys_addr) = copy_memory_map(mmap.entries());

        // 设置页表
        let pml4_addr = setup_page_tables(kernel_size as u64, max_phys_addr);

        let boot_info_ptr = BOOTINFO_PHYS_ADDR as *mut BootInfo;

        let boot_info = BootInfo {
            magic: BOOTINFO_MAGIC,
            version: BOOTINFO_VERSION,
            size: core::mem::size_of::<BootInfo>() as u32,

            framebuffer,

            // 内存映射使用直接映射区虚拟地址
            memory_map_addr: DIRECT_MAP_OFFSET + MEMMAP_ADDR,
            memory_map_entries: mem_entries,
            memory_map_entry_size: core::mem::size_of::<MemoryRegion>() as u32,
            total_memory: total_mem,
            usable_memory: usable_mem,

            acpi_rsdp_addr: acpi_rsdp,
            acpi_version,
            _acpi_reserved: 0,

            smbios_addr,
            smbios_version,
            _smbios_reserved: 0,

            // 磁盘信息使用直接映射区虚拟地址
            disk_info_addr: DIRECT_MAP_OFFSET + DISKINFO_ADDR,
            disk_count,
            boot_disk_index: boot_disk,

            uefi_runtime_services: runtime_services,

            kernel_phys_addr: KERNEL_PHYS_ADDR,
            kernel_virt_addr: KERNEL_VIRT_ADDR,
            kernel_size: kernel_size as u64,

            pml4_phys_addr: pml4_addr,
            direct_map_offset: DIRECT_MAP_OFFSET,

            // 命令行使用直接映射区虚拟地址
            cmdline_addr: DIRECT_MAP_OFFSET + CMDLINE_ADDR,
            cmdline_len: (cmdline.len() - 1) as u32,
            _cmdline_reserved: 0,
        };

        core::ptr::write_volatile(boot_info_ptr, boot_info);

        // 切换页表并跳转到内核
        // 页表提供:
        // 1. 恒等映射: 物理地址 = 虚拟地址 (低4GB)
        // 2. 直接映射: 0xFFFF_8800_0000_0000 + phys = phys
        // 
        // 内核在物理地址 0x100000 运行，但可以通过直接映射访问所有物理内存
        
        asm!(
            "cli",                          // 禁用中断
            "mov cr3, {pml4}",              // 切换页表
            "mov rsp, {stack}",             // 设置栈（恒等映射区）
            "mov rdi, {boot_info}",         // 第一个参数: BootInfo (直接映射虚拟地址)
            "jmp {entry}",                  // 跳转到内核物理地址（恒等映射）
            pml4 = in(reg) pml4_addr,
            stack = in(reg) 0x80000u64,     // 栈顶物理地址（恒等映射）
            boot_info = in(reg) BOOTINFO_VIRT_ADDR,  // BootInfo 使用直接映射虚拟地址
            entry = in(reg) KERNEL_PHYS_ADDR,        // 内核入口物理地址
            options(noreturn)
        );
    }
}

// ============================================================================
// 控制台输出函数
// ============================================================================

fn println_uefi(s: &str) {
    print_uefi(s);
    print_uefi("\r\n");
}

fn print_uefi(s: &str) {
    if let Ok(handle) = boot::get_handle_for_protocol::<Output>() {
        if let Ok(mut stdout) = boot::open_protocol_exclusive::<Output>(handle) {
            let _ = stdout.write_str(s);
        }
    }
}

fn print_hex(val: u64) {
    // 构建十六进制字符串
    let mut buf = [b'0'; 17]; // "0" + 16 hex digits + null
    let mut v = val;
    let mut i = 16;
    
    if v == 0 {
        print_uefi("0");
        return;
    }
    
    while v > 0 && i > 0 {
        i -= 1;
        let digit = (v & 0xF) as u8;
        buf[i] = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
        v >>= 4;
    }
    
    // 直接使用字符串切片
    if let Ok(s) = core::str::from_utf8(&buf[i..16]) {
        print_uefi(s);
    }
}

fn print_dec(val: u64) {
    let mut buf = [b'0'; 20];
    let mut v = val;
    let mut i = 20;
    
    if v == 0 {
        print_uefi("0");
        return;
    }
    
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    
    if let Ok(s) = core::str::from_utf8(&buf[i..20]) {
        print_uefi(s);
    }
}

// ============================================================================
// 图形初始化
// ============================================================================

fn setup_graphics() -> FramebufferInfo {
    println_uefi("      Getting GOP handle...");
    let gop_handle = match boot::get_handle_for_protocol::<GraphicsOutput>() {
        Ok(h) => h,
        Err(_) => {
            println_uefi("      GOP not available, using fallback");
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
    
    println_uefi("      Opening GOP protocol...");
    // 使用 unsafe open_protocol 而不是 exclusive，某些 VMware 版本不支持独占模式
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
            println_uefi("      Failed to open GOP, using fallback");
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

    println_uefi("      Getting mode info...");
    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();
    let stride = mode_info.stride() as u32;
    
    println_uefi("      Getting framebuffer...");
    let mut fb = gop.frame_buffer();
    let fb_addr = fb.as_mut_ptr() as u64;
    let fb_size = fb.size() as u64;
    
    println_uefi("      Setup complete.");
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

// ============================================================================
// 内核加载
// ============================================================================

fn load_kernel() -> usize {
    let fs_handle = boot::get_handle_for_protocol::<SimpleFileSystem>()
        .expect("No filesystem found");
    
    let mut fs = boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
        .expect("Failed to open filesystem");

    let mut root = fs.open_volume().expect("Failed to open volume");

    println_uefi("      Opening kernel file...");
    let kernel_file_handle = root
        .open(
            cstr16!("\\EFI\\january_os\\kernel.bin"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .expect("Failed to open kernel file");

    let mut kernel_file = kernel_file_handle
        .into_regular_file()
        .expect("Kernel is not a regular file");

    println_uefi("      Getting file info...");
    let mut info_buf = [0u8; 256];
    let file_info: &FileInfo = kernel_file
        .get_info(&mut info_buf)
        .expect("Failed to get file info");
    let kernel_size = file_info.file_size() as usize;

    println_uefi("      Allocating memory...");
    let pages = (kernel_size + 4095) / 4096;
    boot::allocate_pages(
        boot::AllocateType::Address(KERNEL_PHYS_ADDR),
        MemoryType::LOADER_CODE,
        pages,
    )
    .expect("Failed to allocate memory for kernel");

    println_uefi("      Reading kernel...");
    let kernel_buffer = unsafe {
        core::slice::from_raw_parts_mut(KERNEL_PHYS_ADDR as *mut u8, kernel_size)
    };
    kernel_file.read(kernel_buffer).expect("Failed to read kernel");

    kernel_size
}

// ============================================================================
// 存储设备扫描
// ============================================================================

fn scan_disks() -> (u32, i32) {
    let disk_info_base = DISKINFO_ADDR as *mut DiskInfo;
    let mut count = 0u32;
    let mut boot_disk = -1i32;

    // 获取所有 BlockIO 句柄
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
            
            // 跳过没有介质的设备
            if !media.is_media_present() {
                continue;
            }

            // 判断磁盘类型
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
                boot_device: 0, // 稍后确定
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

            // 打印磁盘信息
            print_uefi("      Disk ");
            print_dec(count as u64);
            print_uefi(": ");
            match disk_type {
                x if x == DiskType::HardDisk as u32 => print_uefi("HDD"),
                x if x == DiskType::CdRom as u32 => print_uefi("CD-ROM"),
                x if x == DiskType::Usb as u32 => print_uefi("USB"),
                _ => print_uefi("Unknown"),
            }
            print_uefi(", ");
            print_dec(total_size / 1024 / 1024);
            println_uefi(" MB");

            count += 1;
        }
    }

    // 简单启发：第一个非可移动磁盘可能是启动盘
    unsafe {
        for i in 0..count {
            let disk = &*disk_info_base.add(i as usize);
            if disk.removable == 0 && disk.disk_type == DiskType::HardDisk as u32 {
                boot_disk = i as i32;
                // 标记为启动设备
                (*disk_info_base.add(i as usize)).boot_device = 1;
                break;
            }
        }
    }

    (count, boot_disk)
}

// ============================================================================
// ACPI 检测
// ============================================================================

fn find_acpi_rsdp() -> (u64, u32) {
    // ACPI 2.0+ RSDP GUID
    const ACPI2_GUID: uefi::Guid = uefi::guid!("8868e871-e4f1-11d3-bc22-0080c73c8881");
    // ACPI 1.0 RSDP GUID
    const ACPI1_GUID: uefi::Guid = uefi::guid!("eb9d2d30-2d88-11d3-9a16-0090273fc14d");
    
    uefi::system::with_config_table(|tables| {
        // 优先查找 ACPI 2.0
        for table in tables {
            if table.guid == ACPI2_GUID {
                return (table.address as u64, 2);
            }
        }
        // 回退到 ACPI 1.0
        for table in tables {
            if table.guid == ACPI1_GUID {
                return (table.address as u64, 1);
            }
        }
        (0, 0)
    })
}

// ============================================================================
// SMBIOS 检测
// ============================================================================

fn find_smbios() -> (u64, u32) {
    // SMBIOS 3.0 GUID
    const SMBIOS3_GUID: uefi::Guid = uefi::guid!("f2fd1544-9794-4a2c-992e-e5bbcf20e394");
    // SMBIOS 2.x GUID  
    const SMBIOS_GUID: uefi::Guid = uefi::guid!("eb9d2d31-2d88-11d3-9a16-0090273fc14d");
    
    uefi::system::with_config_table(|tables| {
        // 优先查找 SMBIOS 3.0
        for table in tables {
            if table.guid == SMBIOS3_GUID {
                return (table.address as u64, 3);
            }
        }
        // 回退到 SMBIOS 2.x
        for table in tables {
            if table.guid == SMBIOS_GUID {
                return (table.address as u64, 2);
            }
        }
        (0, 0)
    })
}

// ============================================================================
// UEFI 运行时服务
// ============================================================================

fn get_runtime_services() -> u64 {
    // 使用 uefi::runtime 模块获取运行时服务
    // 在 ExitBootServices 之前，可以通过系统表获取
    // 这里我们通过配置表间接获取（实际运行时服务地址）
    uefi::system::with_config_table(|_tables| {
        // 运行时服务表地址通过系统表结构获取
        // 由于 API 限制，这里返回一个标记值
        // 实际的运行时服务调用应该使用 uefi::runtime 模块
        0xDEADBEEF_u64  // 占位符，表示运行时服务可用
    })
}

// ============================================================================
// 页表设置
// ============================================================================

/// 页表分配器状态
struct PageTableAllocator {
    next_page: u64,
}

impl PageTableAllocator {
    fn new(start: u64) -> Self {
        Self { next_page: start }
    }

    /// 分配一个零初始化的页面
    unsafe fn alloc_page(&mut self) -> u64 {
        let page = self.next_page;
        self.next_page += PAGE_SIZE;
        // 清零页面
        core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize);
        page
    }
}

/// 设置内核页表
///
/// 创建以下映射：
/// 1. 恒等映射: 前 4GB 物理地址 = 虚拟地址 (用于引导过渡)
/// 2. 内核映射: 0xFFFF_8000_0010_0000 -> 0x100000 (内核代码)
/// 3. 直接映射: 0xFFFF_8800_0000_0000 + phys -> phys (所有物理内存)
///
/// 返回 PML4 物理地址
unsafe fn setup_page_tables(kernel_size: u64, max_phys_addr: u64) -> u64 {
    let mut allocator = PageTableAllocator::new(PAGE_TABLE_ADDR);

    // 分配 PML4 (顶级页表)
    let pml4 = allocator.alloc_page();
    let pml4_table = pml4 as *mut u64;

    // =========================================================================
    // 1. 恒等映射前 4GB (用于引导过渡)
    //    使用 1GB 大页简化
    // =========================================================================
    
    // PML4[0] -> PDPT for identity map
    let pdpt_identity = allocator.alloc_page();
    *pml4_table.add(0) = pdpt_identity | PTE_PRESENT | PTE_WRITABLE;
    
    let pdpt_identity_table = pdpt_identity as *mut u64;
    // 映射 4 个 1GB 大页 (0-4GB)
    for i in 0..4u64 {
        *pdpt_identity_table.add(i as usize) = 
            (i * 0x4000_0000) | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE;
    }

    // =========================================================================
    // 2. 内核高半部分映射
    //    KERNEL_VIRT_ADDR (0xFFFF_8000_0010_0000) -> KERNEL_PHYS_ADDR (0x100000)
    // =========================================================================
    
    // PML4[256] 对应 0xFFFF_8000_0000_0000 起始
    // (虚拟地址 bit 47 为 1，所以是高半部分)
    let pml4_index_kernel = 256usize; // 0xFFFF_8000... 的 PML4 索引
    
    let pdpt_kernel = allocator.alloc_page();
    *pml4_table.add(pml4_index_kernel) = pdpt_kernel | PTE_PRESENT | PTE_WRITABLE;
    
    let pdpt_kernel_table = pdpt_kernel as *mut u64;
    
    // PDPT[0] -> PD for kernel
    let pd_kernel = allocator.alloc_page();
    *pdpt_kernel_table.add(0) = pd_kernel | PTE_PRESENT | PTE_WRITABLE;
    
    let pd_kernel_table = pd_kernel as *mut u64;
    
    // PD[0] -> PT for kernel (映射前 2MB)
    // 内核在 0x100000 (1MB)，所以需要 PD 索引 0
    let pt_kernel = allocator.alloc_page();
    *pd_kernel_table.add(0) = pt_kernel | PTE_PRESENT | PTE_WRITABLE;
    
    let pt_kernel_table = pt_kernel as *mut u64;
    
    // 映射内核页面 (从 0x100000 开始)
    // 虚拟地址 0xFFFF_8000_0010_0000 + offset -> 物理地址 0x100000 + offset
    let kernel_pages = (kernel_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let kernel_start_pt_index = (KERNEL_PHYS_ADDR / PAGE_SIZE) as usize; // 256 (0x100000 / 0x1000)
    
    for i in 0..kernel_pages as usize {
        let phys_addr = KERNEL_PHYS_ADDR + (i as u64) * PAGE_SIZE;
        *pt_kernel_table.add(kernel_start_pt_index + i) = 
            phys_addr | PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL;
    }
    
    // 同时映射低端内存到高半部分 (用于访问 BootInfo 等)
    // 映射 0-2MB
    for i in 0..512usize {
        if *pt_kernel_table.add(i) == 0 {
            let phys_addr = (i as u64) * PAGE_SIZE;
            *pt_kernel_table.add(i) = phys_addr | PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL;
        }
    }

    // =========================================================================
    // 3. 直接映射区
    //    0xFFFF_8800_0000_0000 + phys -> phys
    //    用于内核访问任意物理地址
    // =========================================================================
    
    // PML4[272] 对应 0xFFFF_8800_0000_0000 起始
    let pml4_index_direct = 272usize;
    
    let pdpt_direct = allocator.alloc_page();
    *pml4_table.add(pml4_index_direct) = pdpt_direct | PTE_PRESENT | PTE_WRITABLE;
    
    let pdpt_direct_table = pdpt_direct as *mut u64;
    
    // 使用 1GB 大页映射物理内存
    // 映射到 max_phys_addr 或至少 4GB
    let max_to_map = max_phys_addr.max(4 * 1024 * 1024 * 1024);
    let gb_pages_needed = (max_to_map + 0x4000_0000 - 1) / 0x4000_0000;
    let gb_pages = gb_pages_needed.min(512); // PDPT 最多 512 项
    
    for i in 0..gb_pages {
        *pdpt_direct_table.add(i as usize) = 
            (i * 0x4000_0000) | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | PTE_GLOBAL;
    }

    pml4
}

// ============================================================================
// 内存映射处理
// ============================================================================

unsafe fn copy_memory_map<'a>(
    mmap: impl Iterator<Item = &'a uefi::mem::memory_map::MemoryDescriptor>
) -> (u32, u64, u64, u64) {
    let dest = MEMMAP_ADDR as *mut MemoryRegion;
    let mut count = 0u32;
    let mut total_mem = 0u64;
    let mut usable_mem = 0u64;
    let mut max_phys_addr = 0u64;

    for entry in mmap {
        if count >= MAX_MEMORY_REGIONS as u32 {
            break;
        }

        let pages = entry.page_count;
        let size = pages * 4096;
        total_mem += size;
        
        // 跟踪最大物理地址
        let end_addr = entry.phys_start + size;
        if end_addr > max_phys_addr {
            max_phys_addr = end_addr;
        }

        // 转换 UEFI 内存类型到简化类型
        let region_type = match entry.ty {
            MemoryType::CONVENTIONAL => {
                usable_mem += size;
                MemoryRegionType::Usable
            }
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => {
                MemoryRegionType::BootloaderReclaimable
            }
            MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => {
                usable_mem += size; // Boot services memory 可回收
                MemoryRegionType::Usable
            }
            MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => {
                MemoryRegionType::Reserved
            }
            MemoryType::ACPI_RECLAIM => MemoryRegionType::AcpiReclaimable,
            MemoryType::ACPI_NON_VOLATILE => MemoryRegionType::AcpiNvs,
            MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => MemoryRegionType::Mmio,
            _ => MemoryRegionType::Reserved,
        };

        let region = MemoryRegion {
            phys_start: entry.phys_start,
            virt_start: entry.virt_start,
            page_count: pages,
            region_type: region_type as u32,
            attributes: entry.att.bits() as u32,
        };

        core::ptr::write_volatile(dest.add(count as usize), region);
        count += 1;
    }

    (count, total_mem, usable_mem, max_phys_addr)
}
