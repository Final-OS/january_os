//! january_os 内核 (x86_64)
//!
//! 这是内核的入口点，从 UEFI 引导程序接收完整的系统信息。

#![no_std]
#![no_main]
#![allow(unsafe_op_in_unsafe_fn)]
// 开发阶段允许的警告（后续逐步修复）
#![allow(dead_code)]
#![allow(unused)]
#![allow(private_interfaces)]
#![allow(non_camel_case_types)]
#![allow(mismatched_lifetime_syntaxes)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(function_casts_as_integer)]
#![allow(clippy::all)]
#![feature(alloc_error_handler)]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;

// 自动生成的配置
mod generated;
pub mod config {
    pub use super::generated::*;
}

// 导入内核库模块
mod arch;
mod drivers;
mod interrupt;
mod mm;
mod sync;

// 使用驱动模块
use drivers::acpi;
use drivers::tty::{serial_init, serial_enable_rx_interrupt, serial_try_read, SerialWriter};
use drivers::tty::fbcon::{self, FbConsoleWriter};

use core::fmt::Write;
use mm::MemoryRegion;

// ============================================================================
// BootInfo 结构体定义
// ============================================================================

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

const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000;

// ============================================================================
// 输出宏 - 同时输出到串口和 Framebuffer 控制台
// ============================================================================

macro_rules! kprint {
    ($($arg:tt)*) => {{
        let _ = write!(SerialWriter, $($arg)*);
        let _ = write!(FbConsoleWriter, $($arg)*);
    }};
}

macro_rules! kprintln {
    () => { kprint!("\n") };
    ($($arg:tt)*) => {{ kprint!($($arg)*); kprint!("\n"); }};
}

// ============================================================================
// 内核入口点
// ============================================================================

// 链接脚本定义的 BSS 段边界
unsafe extern "C" {
    static __bss_start: u8;
    static __bss_end: u8;
}

/// 清零 BSS 段
/// 
/// # Safety
/// 必须在使用任何静态变量之前调用
#[inline(never)]
unsafe fn zero_bss() {
    let start = core::ptr::addr_of!(__bss_start) as *mut u8;
    let end = core::ptr::addr_of!(__bss_end) as *mut u8;
    let size = end as usize - start as usize;
    core::ptr::write_bytes(start, 0, size);
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
pub unsafe extern "C" fn _start(boot_info_ptr: *const BootInfo) -> ! {
    // 必须首先清零 BSS 段
    zero_bss();
    
    serial_init();

    // 验证 BootInfo
    if boot_info_ptr.is_null() {
        panic!("BootInfo pointer is NULL");
    }
    let info = &*boot_info_ptr;
    if info.magic != BOOTINFO_MAGIC {
        panic!("Invalid BootInfo magic: {:#x}", info.magic);
    }

    let direct_map = info.direct_map_offset;

    // 初始化 Framebuffer 控制台（使用虚拟地址 = 物理地址 + direct_map_offset）
    let fb = &info.framebuffer;
    
    // 先用串口输出调试信息
    let _ = write!(SerialWriter, "\n[FB Debug] phys={:#x} size={} {}x{} stride={} fmt={}\n",
        fb.address, fb.size, fb.width, fb.height, fb.stride, fb.pixel_format);
    let _ = write!(SerialWriter, "[FB Debug] direct_map={:#x}\n", direct_map);
    
    if fb.address != 0 && fb.width > 0 && fb.height > 0 {
        let fb_virt_addr = direct_map + fb.address;
        let _ = write!(SerialWriter, "[FB Debug] virt_addr={:#x}\n", fb_virt_addr);
        fbcon::init(
            fb_virt_addr,
            fb.width,
            fb.height,
            fb.stride,
            fb.pixel_format,
        );
    }

    kprintln!();
    kprintln!("================================================================");
    kprintln!("  january_os v0.1.0 - x86_64 Higher-Half Kernel");
    kprintln!("================================================================");
    kprintln!();
    let kernel_end_phys = info.kernel_phys_addr + info.kernel_size;

    // ========================================================================
    // 内存管理初始化
    // ========================================================================
    kprintln!("[1/6] Memory Management");
    
    let mem_regions = info.memory_map_addr as *const MemoryRegion;
    let entries_count = info.memory_map_entries as usize;
    
    // 统计内存
    let mut max_phys_addr: u64 = 0;
    let mut total_usable: u64 = 0;
    for i in 0..entries_count {
        let region = &*mem_regions.add(i);
        let region_end = region.phys_start + region.page_count * 4096;
        if region.region_type == 0 {
            if region_end > max_phys_addr { max_phys_addr = region_end; }
            total_usable += region.page_count * 4096;
        }
    }
    let max_managed = max_phys_addr.min(4 * 1024 * 1024 * 1024);
    let max_pfn = max_managed / 4096;

    // 构建内存区域信息
    const MAX_REGIONS: usize = 64;
    let mut region_infos: [mm::MemoryRegionInfo; MAX_REGIONS] = 
        [mm::MemoryRegionInfo { phys_start: 0, page_count: 0, is_usable: false }; MAX_REGIONS];
    let mut region_info_count = 0usize;
    for i in 0..entries_count.min(MAX_REGIONS) {
        let region = &*mem_regions.add(i);
        region_infos[region_info_count] = mm::MemoryRegionInfo {
            phys_start: region.phys_start,
            page_count: region.page_count,
            is_usable: region.region_type == 0,
        };
        region_info_count += 1;
    }

    // Memblock
    mm::init_memblock(&region_infos[..region_info_count], info.kernel_phys_addr, kernel_end_phys)
        .expect("Memblock init failed");
    
    // Buddy System
    mm::init_buddy_system(&region_infos[..region_info_count], max_pfn, direct_map)
        .expect("Buddy init failed");
    
    // SLUB
    mm::init_slub().expect("SLUB init failed");
    mm::finish_mm_init();
    
    // 初始化堆
    if let Some(heap_page) = mm::alloc_pages(8, mm::GFP_KERNEL) {
        let heap_virt = direct_map + mm::page_to_pfn(heap_page) * 4096;
        mm::init_heap(heap_virt as usize, 256 * 4096);
    }

    // 初始化其他内存组件
    mm::init_pcp(4);
    mm::init_vma();
    mm::init_uma();

    let total_free = mm::ZoneType::iter()
        .map(|zt| mm::get_zone(zt))
        .filter(|z| z.initialized)
        .map(|z| z.nr_free_pages())
        .sum::<u64>();
    
    kprintln!("      Total: {} MB | Free: {} MB | Kernel: {} KB",
        total_usable / 1024 / 1024,
        (total_free * 4) / 1024,
        info.kernel_size / 1024);

    // ========================================================================
    // GDT/TSS 和 IDT
    // ========================================================================
    kprintln!("[2/6] CPU Tables (GDT/IDT)");
    
    let kernel_stack_top: u64;
    unsafe {
        let rsp: u64;
        core::arch::asm!("mov {}, rsp", out(reg) rsp);
        kernel_stack_top = (rsp + 0xFFF) & !0xFFF;
    }
    
    unsafe { interrupt::init_gdt(kernel_stack_top); }
    
    // 分配 IST1 栈
    if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
        let ist_top = direct_map + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
        interrupt::set_interrupt_stack(1, ist_top);
    }
    
    kprintln!("      GDT loaded | TSS configured | IDT ready");

    // ========================================================================
    // ACPI 解析
    // ========================================================================
    kprintln!("[3/6] ACPI Tables");
    
    let mut local_apic_addr: u64 = 0xFEE00000;
    let mut ioapic_addr: u64 = 0;
    let mut ioapic_gsi_base: u32 = 0;
    let mut cpu_count: usize = 1;
    let mut has_iommu = false;

    if info.acpi_rsdp_addr != 0 {
        if acpi::init(info.acpi_rsdp_addr).is_ok() {
            // MADT
            if let Some(madt) = acpi::get_table::<acpi::Madt>(acpi::MADT_SIGNATURE) {
                let madt_info = acpi::parse_madt(madt);
                cpu_count = madt_info.cpu_count;
                local_apic_addr = madt_info.local_apic_address;
                if madt_info.ioapic_count > 0 {
                    ioapic_addr = madt_info.ioapics[0].address as u64;
                    ioapic_gsi_base = madt_info.ioapics[0].gsi_base;
                }
            }
            // DMAR
            if acpi::get_table::<acpi::Dmar>(acpi::DMAR_SIGNATURE).is_some() {
                has_iommu = true;
            }
            kprintln!("      {} CPUs | APIC: {:#x} | IOMMU: {}",
                cpu_count, local_apic_addr, if has_iommu { "yes" } else { "no" });
        } else {
            kprintln!("      ACPI init failed, using defaults");
        }
    } else {
        kprintln!("      ACPI not available");
    }

    // ========================================================================
    // 中断控制器 (APIC)
    // ========================================================================
    kprintln!("[4/6] Interrupt Controller");
    
    let int_info = interrupt::InterruptInitInfo {
        kernel_stack_top,
        local_apic_addr,
        ioapic_addr,
        ioapic_gsi_base,
    };
    
    unsafe {
        interrupt::init(&int_info).expect("Interrupt init failed");
    }
    
    kprintln!("      Local APIC: {:#x} | I/O APIC: {:#x}",
        local_apic_addr, ioapic_addr);

    // ========================================================================
    // IOMMU
    // ========================================================================
    kprintln!("[5/6] IOMMU");
    mm::init_iommu();
    let stats = mm::iommu_stats();
    let iommu_type = match stats.iommu_type {
        mm::IommuType::IntelVtd => "Intel VT-d",
        mm::IommuType::AmdVi => "AMD-Vi",
        mm::IommuType::Swiotlb => "SWIOTLB",
        mm::IommuType::None => "None",
    };
    let trans_mode = match stats.translation_mode {
        mm::TranslationMode::Passthrough => "Passthrough",
        mm::TranslationMode::Translate => "Translate",
    };
    kprintln!("      Type: {} | Mode: {}", iommu_type, trans_mode);

    // ========================================================================
    // Timer 和中断启用
    // ========================================================================
    kprintln!("[6/6] Timer & Interrupts");
    
    let timer_freq = interrupt::calibrate_timer();
    const TIMER_HZ: u32 = 100;
    interrupt::init_apic_timer(interrupt::IRQ_TIMER, TIMER_HZ);
    
    // 启用串口接收中断
    serial_enable_rx_interrupt();
    
    interrupt::enable_interrupts();
    
    kprintln!("      Bus: {} MHz | Timer: {} Hz | Interrupts: ENABLED",
        timer_freq / 1_000_000, TIMER_HZ);

    // ========================================================================
    // 初始化完成
    // ========================================================================
    kprintln!();
    kprintln!("================================================================");
    kprintln!("  Initialization Complete");
    kprintln!("================================================================");
    kprintln!();
    
    // 系统摘要
    kprintln!("System Summary:");
    kprintln!("  Memory:     {} MB total, {} MB free", 
        total_usable / 1024 / 1024, (total_free * 4) / 1024);
    kprintln!("  CPUs:       {} (APIC ID: {})", cpu_count, interrupt::local_apic_id());
    kprintln!("  Disks:      {}", info.disk_count);
    kprintln!();

    // Timer 测试
    kprintln!("Testing timer (3 seconds)...");
    let start = interrupt::timer_ticks();
    while interrupt::timer_ticks() < start + 300 {
        interrupt::halt_with_interrupts();
    }
    kprintln!("  Timer OK: {} ticks", interrupt::timer_ticks() - start);
    kprintln!();

    // 进入交互模式
    kprintln!("Commands: shutdown, status, help");
    kprintln!();
    kprint!("> ");

    // 命令缓冲区
    let mut cmd_buf = [0u8; 64];
    let mut cmd_len = 0usize;

    // 主循环
    loop {
        // 检查键盘输入 (PS/2)
        while let Some(c) = interrupt::read_char() {
            handle_input(c, &mut cmd_buf, &mut cmd_len);
        }
        
        // 检查串口输入 (COM1)
        while let Some(c) = serial_try_read() {
            handle_input(c, &mut cmd_buf, &mut cmd_len);
        }
        
        // 短暂等待
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}

/// 处理输入字符
fn handle_input(c: u8, cmd_buf: &mut [u8; 64], cmd_len: &mut usize) {
    match c {
        8 | 127 => { // Backspace 或 DEL
            if *cmd_len > 0 {
                *cmd_len -= 1;
                kprint!("\x08 \x08");
            }
        }
        b'\n' | b'\r' => {
            kprintln!();
            execute_command(&cmd_buf[..*cmd_len]);
            *cmd_len = 0;
            kprint!("> ");
        }
        3 => { // Ctrl+C
            kprintln!("^C");
            *cmd_len = 0;
            kprint!("> ");
        }
        c if c >= 32 && c < 127 => {
            if *cmd_len < 63 {
                cmd_buf[*cmd_len] = c;
                *cmd_len += 1;
                kprint!("{}", c as char);
            }
        }
        _ => {}
    }
}

/// 执行命令
fn execute_command(cmd: &[u8]) {
    let cmd_str = core::str::from_utf8(cmd).unwrap_or("");
    let cmd_str = cmd_str.trim();
    
    match cmd_str {
        "" => {}
        "shutdown" | "poweroff" => {
            kprintln!("Shutting down...");
            shutdown();
        }
        "reboot" => {
            kprintln!("Rebooting...");
            reboot();
        }
        "status" => {
            kprintln!("Uptime: {} ticks ({} seconds)", 
                interrupt::timer_ticks(),
                interrupt::timer_ticks() / 100);
        }
        "iommu" => {
            let stats = mm::iommu_stats();
            kprintln!("IOMMU Status:");
            kprintln!("  Enabled:     {}", stats.enabled);
            kprintln!("  Type:        {:?}", stats.iommu_type);
            kprintln!("  Translation: {:?}", stats.translation_mode);
            kprintln!("  Units:       {}", stats.nr_units);
            kprintln!("  Mapped:      {} pages", stats.mapped_pages);
        }
        "mem" => {
            let total_free: u64 = mm::ZoneType::iter()
                .map(|zt| unsafe { mm::get_zone(zt) })
                .filter(|z| z.initialized)
                .map(|z| z.nr_free_pages())
                .sum();
            kprintln!("Memory Status:");
            kprintln!("  Free pages:  {}", total_free);
            kprintln!("  Free memory: {} MB", (total_free * 4) / 1024);
        }
        "help" => {
            kprintln!("Commands:");
            kprintln!("  shutdown  - Power off the system");
            kprintln!("  reboot    - Restart the system");
            kprintln!("  status    - Show uptime");
            kprintln!("  iommu     - Show IOMMU status");
            kprintln!("  mem       - Show memory status");
            kprintln!("  help      - Show this help");
        }
        _ => {
            kprintln!("Unknown command: {}", cmd_str);
        }
    }
}

/// ACPI 关机
fn shutdown() -> ! {
    kprintln!("Initiating shutdown...");
    
    // 显示 ACPI 信息
    if let Some((pm1a, pm1b)) = acpi::get_shutdown_info() {
        kprintln!("  PM1a_CNT: {:#x}, PM1b_CNT: {:#x}", pm1a, pm1b);
    }
    
    // 方法1: 使用 ACPI FADT 表（正确方式）
    if let Err(e) = acpi::acpi_shutdown() {
        kprintln!("ACPI shutdown failed: {}", e);
    }
    
    unsafe {
        // 方法2: QEMU 调试端口
        core::arch::asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16);
        
        // 方法3: Bochs/老版 QEMU
        core::arch::asm!("out dx, ax", in("dx") 0xB004u16, in("ax") 0x2000u16);
    }
    
    kprintln!("Shutdown failed, halting CPU...");
    halt();
}

/// 重启
fn reboot() -> ! {
    unsafe {
        // 方法 1: ACPI 重启 (QEMU 支持)
        // 通过 I/O 端口 0xCF9 发送重启命令
        core::arch::asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8);
        
        // 方法 2: 8042 键盘控制器重启
        for _ in 0..10 {
            core::arch::asm!(
                "in al, 0x64",
                "test al, 0x02",
                "jnz 2f",
                "mov al, 0xFE",
                "out 0x64, al",
                "2:",
                out("al") _,
            );
        }
        
        // 方法 3: Triple Fault (最后手段)
        // 加载空的 IDT 然后触发中断
        let null_idt: [u8; 6] = [0; 6];
        core::arch::asm!(
            "lidt [{}]",
            "int3",
            in(reg) &null_idt,
        );
    }
    halt();
}

fn halt() -> ! {
    loop { unsafe { asm!("hlt"); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!();
    kprintln!("!!! KERNEL PANIC !!!");
    if let Some(loc) = info.location() {
        kprintln!("  {}:{}", loc.file(), loc.line());
    }
    if let Some(msg) = info.message().as_str() {
        kprintln!("  {}", msg);
    }
    halt();
}
