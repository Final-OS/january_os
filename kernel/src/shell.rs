//! 交互式 Shell
//!
//! 提供基本的命令行交互功能。

use crate::{kprintln, kprint, info};
use crate::drivers::{self, acpi};
use crate::interrupt;
use crate::mm;
use crate::config;
use crate::drivers::tty::{serial_read_char};
use crate::drivers::input::hid::keyboard;
use crate::arch::{shutdown, reboot};
use core::arch::asm;

/// 进入 Shell 主循环
pub fn run() -> ! {
    kprintln!("Commands: shutdown, status, help");
    kprintln!();
    kprint!("> ");

    // 命令缓冲区
    let mut cmd_buf = [0u8; 256];
    let mut cmd_len = 0usize;

    // 主循环
    loop {
        let mut activity = false;

        // 检查键盘输入 (PS/2)
        while let Some(c) = interrupt::read_char() {
            handle_input(c, &mut cmd_buf, &mut cmd_len);
            activity = true;
        }
        
        // 检查串口输入 (COM1)
        while let Some(c) = serial_read_char() {
            handle_input(c, &mut cmd_buf, &mut cmd_len);
            activity = true;
        }

        // 检查 USB 键盘输入
        while let Some(c) = keyboard::read_char() {
            handle_input(c, &mut cmd_buf, &mut cmd_len);
            activity = true;
        }
        
        // 如果没有活动，挂起 CPU 直到下一个中断 (HLT)
        // 这样可以降低 CPU 占用率和功耗
        // 任何中断（如时钟、键盘、USB）都会唤醒 CPU
        if !activity {
            interrupt::halt_with_interrupts();
        }
    }
}

/// 处理输入字符
fn handle_input(c: u8, cmd_buf: &mut [u8; 256], cmd_len: &mut usize) {
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
            if *cmd_len < 255 {
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
    
    let mut parts = cmd_str.split_whitespace();
    let command = parts.next().unwrap_or("");
    let args = parts;

    match command {
        "" => {}
        "shutdown" | "poweroff" => {
            info!("Shutting down...");
            shutdown();
        }
        "reboot" => {
            info!("Rebooting...");
            reboot();
        }
        "status" => {
            kprintln!("Uptime: {} ticks ({} seconds)", 
                interrupt::timer_ticks(),
                interrupt::timer_ticks() / 100);
        }
        "mm" => {
            execute_mm_command(args);
        }
        "drivers" => {
            execute_drivers_command(args);
        }
        "pci" => {
            execute_pci_command();
        }
        "usb" => {
            execute_usb_command();
        }
        "test" => {
            execute_test_command(args);
        }
        "help" => {
            kprintln!("Available commands:");
            kprintln!("  shutdown   - Power off system");
            kprintln!("  reboot     - Restart system");
            kprintln!("  status     - Show system status");
            kprintln!("  drivers    - Driver management commands");
            kprintln!("  mm         - Memory management commands");
            kprintln!("  pci        - Show PCI devices");
            kprintln!("  usb        - Show USB devices");
            kprintln!("  test       - Run system tests");
            kprintln!("  help       - Show this help message");
            kprintln!("Type 'drivers help' or 'mm help' for more info.");
        }
        _ => {
            kprintln!("Unknown command: '{}'. Type 'help' for available commands.", command);
        }
    }
}

fn execute_mm_command(mut args: core::str::SplitWhitespace) {
    let subcommand = args.next().unwrap_or("help");
    
    match subcommand {
        "status" => {
            let total_free: u64 = mm::ZoneType::iter()
                .map(|zt| unsafe { mm::get_zone(zt) })
                .filter(|z| z.initialized)
                .map(|z| z.nr_free_pages())
                .sum();
            kprintln!("Memory Status:");
            kprintln!("  Free pages:  {}", total_free);
            kprintln!("  Heap size:   {} MB", config::KERNEL_HEAP_INIT_SIZE / 1024 / 1024);
        }
        "memblock" => {
            kprintln!("Memblock Status:");
            kprintln!("  Phys Mem Size: {} bytes", mm::memblock_phys_mem_size());
            kprintln!("  Reserved Size: {} bytes", mm::memblock_reserved_size());
            kprintln!("  Free Size:     {} bytes", mm::memblock_free_size());
            
            kprintln!("Memory Regions:");
            for i in 0..mm::memblock_memory_region_count() {
                if let Some(region) = mm::memblock_memory_region(i) {
                     kprintln!("  [{}] {:#x} - {:#x} ({} bytes)", 
                        i, region.base, region.end(), region.size);
                }
            }
            
            kprintln!("Reserved Regions:");
            for i in 0..mm::memblock_reserved_region_count() {
                if let Some(region) = mm::memblock_reserved_region(i) {
                     kprintln!("  [{}] {:#x} - {:#x} ({} bytes)", 
                        i, region.base, region.end(), region.size);
                }
            }
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
        "help" | _ => {
            kprintln!("Usage: mm <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  status    - Show general memory usage");
            kprintln!("  memblock  - Show early memory map (memblock)");
            kprintln!("  iommu     - Show IOMMU status");
        }
    }
}

fn execute_drivers_command(mut args: core::str::SplitWhitespace) {
    let subcommand = args.next().unwrap_or("help");
    
    match subcommand {
        "acpi" => {
            kprintln!("ACPI Tables:");
            drivers::acpi::dump_tables();
        }
        "cpu" => {
            if let Some(madt) = drivers::acpi::find_table::<drivers::acpi::Madt>() {
                kprintln!("CPU Information (from MADT):");
                kprintln!("  Local APIC Address: {:#x}", madt.local_apic_addr());
                kprintln!("  Has 8259 PIC: {}", madt.has_8259_pic());
                
                let mut cpu_count = 0;
                for entry in madt.entries() {
                    if let drivers::acpi::MadtEntry::LocalApic(lapic) = entry {
                        kprintln!("  CPU #{}: APIC ID={}, Enabled={}, OnlineCapable={}", 
                            cpu_count, lapic.apic_id, lapic.is_enabled(), lapic.is_online_capable());
                        cpu_count += 1;
                    }
                }
                kprintln!("  Total CPUs found: {}", cpu_count);
            } else {
                kprintln!("MADT table not found!");
            }
        }
        "video" => {
            let (w, h, s, f) = drivers::tty::fbcon::info();
            kprintln!("Video Status:");
            kprintln!("  Resolution:  {}x{}", w, h);
            kprintln!("  Stride:      {} pixels", s);
            kprintln!("  Format:      {:?} ({})", 
                if f == 0 { "RGB" } else if f == 1 { "BGR" } else { "Other" }, f);
        }
        "input" => {
            kprintln!("Input Devices Status:");
            kprintln!("  PS/2 Mouse ID: {:#x}", drivers::input::mouse_device_id());
            kprintln!("  PS/2 Keyboard: Initialized");

            // HID Status
            kprintln!("  HID Keyboard:  {}", if drivers::input::hid::keyboard::is_present() { "Present" } else { "Not Present" });
            let (k_head, k_tail) = drivers::input::hid::keyboard::buffer_status();
            kprintln!("    Buffer: {}/{} (Head/Tail)", k_head, k_tail);

            kprintln!("  HID Mouse:     {}", if drivers::input::hid::mouse::is_present() { "Present" } else { "Not Present" });
            let (m_head, m_tail) = drivers::input::hid::mouse::buffer_status();
            kprintln!("    Buffer: {}/{} (Head/Tail)", m_head, m_tail);
        }
        "mouse" => {
            kprintln!("Mouse Test Mode (ID: {:#x}) (Press any key to exit)", drivers::input::mouse_device_id());
            let mut last_count = drivers::input::mouse_event_count();
            while interrupt::read_char().is_none() {
                let current_count = drivers::input::mouse_event_count();
                if current_count > last_count {
                     last_count = current_count;
                     let dx = drivers::input::delta_x();
                     let dy = drivers::input::delta_y();
                     let l = drivers::input::left_button();
                     let r = drivers::input::right_button();
                     let m = drivers::input::middle_button();
                     kprintln!("[{}] Mouse: X={:<4} Y={:<4} L={} M={} R={}", current_count, dx, dy, l, m, r);
                }
                // 简单的防抖/延时
                for _ in 0..1000 { core::hint::spin_loop(); }
            }
            kprintln!("Exited mouse test mode.");
        }
        "interrupt" => {
            let int_enabled = interrupt::interrupts_enabled();
            kprintln!("Interrupt Status:");
            kprintln!("  CPU Interrupts: {}", if int_enabled { "Enabled" } else { "Disabled" });
            kprintln!("  Local APIC ID:  {}", interrupt::local_apic_id());
            kprintln!("  Timer Ticks:    {}", interrupt::timer_ticks());
            
            kprintln!("IDT Vectors:");
            kprintln!("  Timer:    {}", interrupt::IRQ_TIMER);
            kprintln!("  Keyboard: {}", interrupt::IRQ_KEYBOARD);
            kprintln!("  Mouse:    {}", interrupt::IRQ_MOUSE);
            kprintln!("  COM1:     {}", interrupt::IRQ_COM1);
        }
        "help" | _ => {
            kprintln!("Usage: drivers <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  acpi      - Show ACPI tables");
            kprintln!("  cpu       - Show CPU information");
            kprintln!("  video     - Show video status");
            kprintln!("  input     - Show input devices status");
            kprintln!("  mouse     - Mouse test mode");
            kprintln!("  interrupt - Show interrupt status");
        }
    }
}

fn execute_pci_command() {
    kprintln!("PCI Devices:");
    drivers::pci::scan_bus(&mut |addr, header| {
         kprintln!("  {:02x}:{:02x}.{} {:04x}:{:04x} Class {:02x} Sub {:02x} ProgIF {:02x}",
             addr.bus, addr.device, addr.function,
             header.vendor_id, header.device_id,
             header.class_code, header.subclass, header.prog_if);
    });
}

fn execute_usb_command() {
    drivers::usb::xhci::dump_devices();
}

fn execute_test_command(mut args: core::str::SplitWhitespace) {
    let subcommand = args.next().unwrap_or("help");

    match subcommand {
        "timer" => {
            info!("Testing timer (3 seconds)...");
            let start = interrupt::timer_ticks();
            while interrupt::timer_ticks() < start + 300 {
                interrupt::halt_with_interrupts();
            }
            crate::ok!("Timer test passed: {} ticks", interrupt::timer_ticks() - start);
        }
        other => crate::tests::run(other),
    }
}
