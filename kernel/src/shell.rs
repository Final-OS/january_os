//! 交互式 Shell
//!
//! 提供基本的命令行交互功能。

use crate::{kprintln, kprint, info};
use crate::drivers;
use crate::interrupt;
use crate::mm;
use crate::config;
use crate::drivers::tty::{serial_read_char};
use crate::task;
use crate::drivers::input::hid::keyboard;
use crate::arch::{shutdown, reboot};
use core::arch::asm;

/// 进入 Shell 主循环
pub fn run() -> ! {
    kprintln!("Commands: shutdown, status, runuser, hotkey, help");
    kprintln!();
    kprint!("> ");

    // 命令缓冲区
    let mut cmd_buf = [0u8; 256];
    let mut cmd_len = 0usize;

    // 主循环
    loop {
        let mut activity = false;

        // 轮询输入后端（例如 USB/xHCI 事件环）。
        drivers::input::poll();

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

        // 检查 ACPI 热键事件
        while let Some(event) = drivers::input::read_hotkey_event() {
            handle_hotkey_event(event);
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

extern "C" fn run_user_demo_task() {
    const DEMO_PATH: &str = "/bin/demo_user";

    let image = match task::lookup_builtin_exec_image(DEMO_PATH) {
        Some(image) => image,
        None => {
            crate::kprintln!("[diag][user] demo image missing path={}", DEMO_PATH);
            task::exit_current_task(127);
            task::scheduler::schedule();
            loop {
                core::hint::spin_loop();
            }
        }
    };

    let load_plan = match task::build_elf_load_plan(image) {
        Ok(plan) => plan,
        Err(errno) => {
            crate::kprintln!(
                "[diag][user] build load plan failed path={} errno={}",
                DEMO_PATH,
                errno
            );
            task::exit_current_task(127);
            task::scheduler::schedule();
            loop {
                core::hint::spin_loop();
            }
        }
    };

    let map_preview = task::preview_pt_load_mapping(&load_plan);
    let stack_bytes = map_preview.stack_pages.saturating_mul(mm::PAGE_SIZE);
    let stack_bottom = load_plan.stack_top.saturating_sub(stack_bytes);

    crate::kprintln!(
        "[diag][user] load plan path={} entry={:#x} segs={} seg_pages={} stack=[{:#x}, {:#x}) total_pages={}",
        DEMO_PATH,
        load_plan.entry,
        map_preview.segment_count,
        map_preview.segment_pages,
        stack_bottom,
        load_plan.stack_top,
        map_preview.total_pages,
    );

    let staged_mappings = match task::stage_pt_load_mappings(image, &load_plan) {
        Ok(mappings) => mappings,
        Err(errno) => {
            crate::kprintln!(
                "[diag][user] stage mappings failed path={} errno={} segs={} seg_pages={} stack=[{:#x}, {:#x}) total_pages={}",
                DEMO_PATH,
                errno,
                map_preview.segment_count,
                map_preview.segment_pages,
                stack_bottom,
                load_plan.stack_top,
                map_preview.total_pages,
            );
            task::exit_current_task(127);
            task::scheduler::schedule();
            loop {
                core::hint::spin_loop();
            }
        }
    };

    let staged_pages = staged_mappings.len();
    let staged_first = staged_mappings.first().map(|page| page.virt).unwrap_or(0);
    crate::kprintln!(
        "[diag][user] stage mappings ok path={} pages={} first_virt={:#x}",
        DEMO_PATH,
        staged_pages,
        staged_first,
    );

    if task::record_current_exec_request(DEMO_PATH, 1, 0).is_none() {
        crate::kprintln!(
            "[diag][user] record exec request failed path={} -> rollback staged mappings",
            DEMO_PATH
        );
        task::rollback_exec_mappings(&staged_mappings);
        task::exit_current_task(127);
        task::scheduler::schedule();
        loop {
            core::hint::spin_loop();
        }
    }

    let replaced_pages = match task::set_current_exec_mappings(staged_mappings) {
        Some(replaced) => replaced,
        None => {
            crate::kprintln!(
                "[diag][user] set current exec mappings failed path={}",
                DEMO_PATH
            );
            task::exit_current_task(127);
            task::scheduler::schedule();
            loop {
                core::hint::spin_loop();
            }
        }
    };

    crate::kprintln!(
        "[diag][user] exec mappings installed path={} staged_pages={} replaced_pages={}",
        DEMO_PATH,
        staged_pages,
        replaced_pages,
    );

    let user_frame = task::arch::build_user_enter_frame(load_plan.entry, load_plan.stack_top);
    crate::kprintln!(
        "[diag][user] enter ring3 path={} rip={:#x} rsp={:#x}",
        DEMO_PATH,
        user_frame.rip,
        user_frame.rsp
    );

    unsafe {
        task::arch::enter_user_mode_iret(&user_frame);
    }
}

fn execute_runuser_command() {
    let demo_task = task::spawn_kernel_thread("user_demo", run_user_demo_task);
    let demo_pid = demo_task.lock().pid;

    crate::kprintln!(
        "[diag][shell] spawned user demo pid={} path=/bin/demo_user",
        demo_pid.0
    );

    // 切换到新任务；当用户任务退出后，会回到 shell 空闲上下文。
    task::scheduler::schedule();

    crate::kprintln!(
        "[diag][shell] user demo returned to shell pid={}",
        demo_pid.0
    );
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
        "runuser" => {
            execute_runuser_command();
        }
        "hotkey" => {
            execute_hotkey_command(args);
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
            kprintln!("  runuser    - Launch built-in ring3 demo");
            kprintln!("  hotkey     - ACPI hotkey debug commands");
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
            let fault_stats = mm::get_fault_stats();
            kprintln!("Memory Status:");
            kprintln!("  Free pages:  {}", total_free);
            kprintln!("  Heap size:   {} MB", config::KERNEL_HEAP_INIT_SIZE / 1024 / 1024);
            kprintln!(
                "  Faults:      total={} minor={} major={} cow={} stack_grow={}",
                fault_stats.total_faults.load(core::sync::atomic::Ordering::Relaxed),
                fault_stats.minor_faults.load(core::sync::atomic::Ordering::Relaxed),
                fault_stats.major_faults.load(core::sync::atomic::Ordering::Relaxed),
                fault_stats.cow_faults.load(core::sync::atomic::Ordering::Relaxed),
                fault_stats.stack_grows.load(core::sync::atomic::Ordering::Relaxed),
            );
        }
        "faults" => {
            let fault_stats = mm::get_fault_stats();
            kprintln!("Page Fault Stats:");
            kprintln!(
                "  total_faults: {}",
                fault_stats.total_faults.load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  minor_faults: {}",
                fault_stats.minor_faults.load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  major_faults: {}",
                fault_stats.major_faults.load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  cow_faults:   {}",
                fault_stats.cow_faults.load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  stack_grows:  {}",
                fault_stats.stack_grows.load(core::sync::atomic::Ordering::Relaxed)
            );
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
            kprintln!("  faults    - Show page-fault statistics");
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

            let (h_head, h_tail) = drivers::input::hotkey_buffer_status();
            kprintln!("  ACPI Hotkey:   {}", if drivers::input::has_hotkey_event() { "Pending" } else { "Empty" });
            kprintln!("    Buffer: {}/{} (Head/Tail)", h_head, h_tail);
            if let Some(src) = drivers::input::hotkey_source_info() {
                kprintln!(
                    "    Source: pm1a={:#x} pm1b={:#x} sts_len={}",
                    src.pm1a_evt_port,
                    src.pm1b_evt_port,
                    src.pm1_sts_len,
                );
            } else {
                kprintln!("    Source: unavailable");
            }
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

fn execute_hotkey_command(mut args: core::str::SplitWhitespace) {
    let subcommand = args.next().unwrap_or("help");

    match subcommand {
        "status" => {
            let (head, tail) = drivers::input::hotkey_buffer_status();
            kprintln!("ACPI Hotkey Status:");
            kprintln!("  Pending: {}", drivers::input::has_hotkey_event());
            kprintln!("  Buffer:  {}/{} (Head/Tail)", head, tail);
            if let Some(src) = drivers::input::hotkey_source_info() {
                kprintln!("  Source:");
                kprintln!("    PM1A_EVT: {:#x}", src.pm1a_evt_port);
                kprintln!("    PM1B_EVT: {:#x}", src.pm1b_evt_port);
                kprintln!("    PM1_STS_LEN: {}", src.pm1_sts_len);
            } else {
                kprintln!("  Source: unavailable");
            }
        }
        "read" => {
            let mut drained = 0usize;
            while let Some(event) = drivers::input::read_hotkey_event() {
                drained += 1;
                kprintln!("  [{}] {}", drained, hotkey_event_name(event));
            }
            if drained == 0 {
                kprintln!("No ACPI hotkey events.");
            }
        }
        "inject" => {
            let name = args.next().unwrap_or("");
            match parse_hotkey_name(name) {
                Some(event) => {
                    drivers::input::inject_hotkey_event(event);
                    kprintln!("Injected ACPI hotkey event: {}", hotkey_event_name(event));
                }
                None => {
                    kprintln!("Invalid hotkey name: '{}'", name);
                    kprintln!("Valid names: power, sleep, hibernate, volup, voldown, mute, briup, bridown");
                }
            }
        }
        "help" | _ => {
            kprintln!("Usage: hotkey <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  status              - Show ACPI hotkey queue status");
            kprintln!("  read                - Drain and print queued hotkey events");
            kprintln!("  inject <event>      - Inject a test hotkey event");
            kprintln!("Events: power, sleep, hibernate, volup, voldown, mute, briup, bridown");
        }
    }
}

fn parse_hotkey_name(name: &str) -> Option<drivers::input::HotkeyEvent> {
    match name {
        "power" | "pwrbtn" | "powerbtn" => Some(drivers::input::HotkeyEvent::PowerButton),
        "sleep" => Some(drivers::input::HotkeyEvent::Sleep),
        "hibernate" => Some(drivers::input::HotkeyEvent::Hibernate),
        "volup" | "volumeup" => Some(drivers::input::HotkeyEvent::VolumeUp),
        "voldown" | "volumedown" => Some(drivers::input::HotkeyEvent::VolumeDown),
        "mute" | "volumemute" => Some(drivers::input::HotkeyEvent::VolumeMute),
        "briup" | "brightnessup" => Some(drivers::input::HotkeyEvent::BrightnessUp),
        "bridown" | "brightnessdown" => Some(drivers::input::HotkeyEvent::BrightnessDown),
        _ => None,
    }
}

fn hotkey_event_name(event: drivers::input::HotkeyEvent) -> &'static str {
    match event {
        drivers::input::HotkeyEvent::PowerButton => "power_button",
        drivers::input::HotkeyEvent::Sleep => "sleep",
        drivers::input::HotkeyEvent::Hibernate => "hibernate",
        drivers::input::HotkeyEvent::VolumeUp => "volume_up",
        drivers::input::HotkeyEvent::VolumeDown => "volume_down",
        drivers::input::HotkeyEvent::VolumeMute => "volume_mute",
        drivers::input::HotkeyEvent::BrightnessUp => "brightness_up",
        drivers::input::HotkeyEvent::BrightnessDown => "brightness_down",
    }
}

fn handle_hotkey_event(event: drivers::input::HotkeyEvent) {
    kprintln!("[hotkey] event={}", hotkey_event_name(event));
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
