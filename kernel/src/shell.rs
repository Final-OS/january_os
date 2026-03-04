//! 交互式 Shell
//!
//! 提供基本的命令行交互功能。

use crate::arch::{reboot, shutdown};
use crate::config;
use crate::drivers;
use crate::drivers::input::hid::keyboard;
use crate::drivers::tty::serial_read_char;
use crate::interrupt;
use crate::mm;
use crate::{info, kprint, kprintln};
use alloc::string::String;
use alloc::vec::Vec;

const CMD_BUF_SIZE: usize = 256;
const HISTORY_SIZE: usize = 16;
const SHELL_COMMANDS: [&str; 11] = [
    "shutdown", "poweroff", "reboot", "status", "mm", "drivers", "pci", "usb", "test", "hotkey",
    "help",
];

#[derive(Clone, Copy)]
enum EscapeState {
    None,
    Esc,
    Csi,
    Ss3,
}

struct ShellState {
    cmd_buf: [u8; CMD_BUF_SIZE],
    cmd_len: usize,
    esc_state: EscapeState,

    history: [[u8; CMD_BUF_SIZE]; HISTORY_SIZE],
    history_len: [usize; HISTORY_SIZE],
    history_head: usize,
    history_count: usize,
    history_cursor: Option<usize>,
    draft_buf: [u8; CMD_BUF_SIZE],
    draft_len: usize,
}

impl ShellState {
    const fn new() -> Self {
        Self {
            cmd_buf: [0; CMD_BUF_SIZE],
            cmd_len: 0,
            esc_state: EscapeState::None,
            history: [[0; CMD_BUF_SIZE]; HISTORY_SIZE],
            history_len: [0; HISTORY_SIZE],
            history_head: 0,
            history_count: 0,
            history_cursor: None,
            draft_buf: [0; CMD_BUF_SIZE],
            draft_len: 0,
        }
    }

    fn history_slot(&self, logical_index: usize) -> usize {
        (self.history_head + logical_index) % HISTORY_SIZE
    }

    fn exit_history_browse(&mut self) {
        self.history_cursor = None;
        self.draft_len = 0;
    }
}

fn print_prompt() {
    kprint!("> ");
}

fn redraw_line(state: &ShellState, previous_len: usize) {
    kprint!("\r> ");
    for &b in &state.cmd_buf[..state.cmd_len] {
        kprint!("{}", b as char);
    }

    if previous_len > state.cmd_len {
        let clear = previous_len - state.cmd_len;
        for _ in 0..clear {
            kprint!(" ");
        }
        for _ in 0..clear {
            kprint!("\x08");
        }
    }
}

fn copy_into(dst: &mut [u8], src: &[u8], len: usize) {
    let mut i = 0usize;
    while i < len {
        dst[i] = src[i];
        i += 1;
    }
}

fn bytes_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn trim_command_bytes(cmd: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = cmd.len();

    while start < end && cmd[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && cmd[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &cmd[start..end]
}

fn save_history(state: &mut ShellState, cmd: &[u8]) {
    let cmd = trim_command_bytes(cmd);
    if cmd.is_empty() {
        return;
    }

    if state.history_count > 0 {
        let last_idx = state.history_slot(state.history_count - 1);
        let last_len = state.history_len[last_idx];
        if last_len == cmd.len() && bytes_equal(&state.history[last_idx][..last_len], cmd) {
            return;
        }
    }

    let slot = if state.history_count < HISTORY_SIZE {
        let s = state.history_slot(state.history_count);
        state.history_count += 1;
        s
    } else {
        let s = state.history_head;
        state.history_head = (state.history_head + 1) % HISTORY_SIZE;
        s
    };

    state.history_len[slot] = cmd.len();
    copy_into(&mut state.history[slot], cmd, cmd.len());
}

fn load_history_entry(state: &mut ShellState, logical_index: usize) {
    let slot = state.history_slot(logical_index);
    let len = state.history_len[slot];
    state.cmd_len = len;
    copy_into(&mut state.cmd_buf, &state.history[slot], len);
}

fn history_up(state: &mut ShellState) {
    if state.history_count == 0 {
        return;
    }

    let previous_len = state.cmd_len;
    let next_cursor = match state.history_cursor {
        None => {
            state.draft_len = state.cmd_len;
            copy_into(&mut state.draft_buf, &state.cmd_buf, state.cmd_len);
            state.history_count - 1
        }
        Some(0) => 0,
        Some(c) => c - 1,
    };

    state.history_cursor = Some(next_cursor);
    load_history_entry(state, next_cursor);
    redraw_line(state, previous_len);
}

fn history_down(state: &mut ShellState) {
    let Some(cursor) = state.history_cursor else {
        return;
    };

    let previous_len = state.cmd_len;
    if cursor + 1 < state.history_count {
        let next = cursor + 1;
        state.history_cursor = Some(next);
        load_history_entry(state, next);
    } else {
        state.history_cursor = None;
        state.cmd_len = state.draft_len;
        copy_into(&mut state.cmd_buf, &state.draft_buf, state.draft_len);
        state.draft_len = 0;
    }
    redraw_line(state, previous_len);
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut i = 0usize;
    let limit = a_bytes.len().min(b_bytes.len());
    while i < limit {
        if a_bytes[i] != b_bytes[i] {
            break;
        }
        i += 1;
    }
    i
}

fn set_line_from_str(state: &mut ShellState, line: &str) {
    let bytes = line.as_bytes();
    let len = bytes.len().min(CMD_BUF_SIZE - 1);
    state.cmd_len = len;
    copy_into(&mut state.cmd_buf, bytes, len);
}

fn complete_command(state: &mut ShellState) {
    if state.cmd_len == 0 {
        kprintln!();
        for &cmd in &SHELL_COMMANDS {
            kprint!("{}  ", cmd);
        }
        kprintln!();
        print_prompt();
        return;
    }

    if state.cmd_buf[..state.cmd_len]
        .iter()
        .any(|&b| b.is_ascii_whitespace())
    {
        return;
    }

    let prefix_len = state.cmd_len;
    let mut prefix_buf = [0u8; CMD_BUF_SIZE];
    copy_into(&mut prefix_buf, &state.cmd_buf, prefix_len);
    let prefix = &prefix_buf[..prefix_len];

    let mut match_count = 0usize;
    let mut first_match = "";
    let mut lcp = "";

    for &cmd in &SHELL_COMMANDS {
        if cmd.as_bytes().starts_with(prefix) {
            if match_count == 0 {
                first_match = cmd;
                lcp = cmd;
            } else {
                let n = common_prefix_len(lcp, cmd);
                lcp = &lcp[..n];
            }
            match_count += 1;
        }
    }

    if match_count == 0 {
        return;
    }

    let previous_len = state.cmd_len;
    state.exit_history_browse();

    if match_count == 1 {
        let mut completed = first_match;
        if completed.len() < CMD_BUF_SIZE - 1 {
            set_line_from_str(state, completed);
            if state.cmd_len < CMD_BUF_SIZE - 1 {
                state.cmd_buf[state.cmd_len] = b' ';
                state.cmd_len += 1;
            }
        } else {
            completed = &completed[..CMD_BUF_SIZE - 1];
            set_line_from_str(state, completed);
        }
        redraw_line(state, previous_len);
        return;
    }

    if lcp.len() > prefix_len {
        set_line_from_str(state, lcp);
        redraw_line(state, previous_len);
        return;
    }

    kprintln!();
    for &cmd in &SHELL_COMMANDS {
        if cmd.as_bytes().starts_with(prefix) {
            kprint!("{}  ", cmd);
        }
    }
    kprintln!();
    print_prompt();
    for &b in &state.cmd_buf[..state.cmd_len] {
        kprint!("{}", b as char);
    }
}

/// 进入 Shell 主循环
pub fn run() -> ! {
    kprintln!("Commands: shutdown, status, test, hotkey, help");
    kprintln!("Shortcuts: Tab=complete, Up/Down=history");
    kprintln!();
    print_prompt();

    let mut shell = ShellState::new();

    // 主循环
    loop {
        let mut activity = false;

        // 轮询输入后端（例如 USB/xHCI 事件环）。
        drivers::input::poll();

        // 检查键盘输入 (PS/2)
        while let Some(c) = interrupt::read_char() {
            handle_input(c, &mut shell);
            activity = true;
        }

        // 检查串口输入 (COM1)
        while let Some(c) = serial_read_char() {
            handle_input(c, &mut shell);
            activity = true;
        }

        // 检查 USB 键盘输入
        while let Some(c) = keyboard::read_char() {
            handle_input(c, &mut shell);
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

/// 处理输入字符
fn handle_input(c: u8, state: &mut ShellState) {
    match state.esc_state {
        EscapeState::None => {}
        EscapeState::Esc => {
            state.esc_state = match c {
                b'[' => EscapeState::Csi,
                b'O' => EscapeState::Ss3,
                _ => EscapeState::None,
            };
            return;
        }
        EscapeState::Csi | EscapeState::Ss3 => {
            state.esc_state = EscapeState::None;
            match c {
                b'A' => history_up(state),
                b'B' => history_down(state),
                _ => {}
            }
            return;
        }
    }

    match c {
        0x1b => {
            state.esc_state = EscapeState::Esc;
        }
        8 | 127 => {
            if state.cmd_len > 0 {
                state.exit_history_browse();
                state.cmd_len -= 1;
                kprint!("\x08 \x08");
            }
        }
        b'\t' => {
            complete_command(state);
        }
        b'\n' | b'\r' => {
            kprintln!();
            let line_len = state.cmd_len;
            let mut line_buf = [0u8; CMD_BUF_SIZE];
            copy_into(&mut line_buf, &state.cmd_buf, line_len);
            save_history(state, &line_buf[..line_len]);
            execute_command(&line_buf[..line_len]);
            state.cmd_len = 0;
            state.esc_state = EscapeState::None;
            state.exit_history_browse();
            print_prompt();
        }
        3 => {
            kprintln!("^C");
            state.cmd_len = 0;
            state.esc_state = EscapeState::None;
            state.exit_history_browse();
            print_prompt();
        }
        c if c.is_ascii_graphic() || c == b' ' => {
            if state.cmd_len < CMD_BUF_SIZE - 1 {
                state.exit_history_browse();
                state.cmd_buf[state.cmd_len] = c;
                state.cmd_len += 1;
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

    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    let command = parts.first().copied().unwrap_or("");
    let args = if parts.len() > 1 { &parts[1..] } else { &[] };

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
            kprintln!(
                "Uptime: {} ticks ({} seconds)",
                interrupt::timer_ticks(),
                interrupt::timer_ticks() / 100
            );
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
            kprintln!("  hotkey     - ACPI hotkey debug commands");
            kprintln!("  help       - Show this help message");
            kprintln!("Shortcuts: Tab=complete command, Up/Down=browse command history");
            kprintln!("Type 'drivers help' or 'mm help' for more info.");
        }
        _ => {
            kprintln!(
                "Unknown command: '{}'. Type 'help' for available commands.",
                command
            );
        }
    }
}

fn execute_mm_command(args: &[&str]) {
    let subcommand = args.first().copied().unwrap_or("help");

    match subcommand {
        "status" => {
            let total_free: u64 = mm::ZoneType::iter()
                .map(mm::get_zone)
                .filter(|z| z.initialized)
                .map(|z| z.nr_free_pages())
                .sum();
            let fault_stats = mm::get_fault_stats();
            let (vmalloc_heal_ok, vmalloc_heal_miss) = mm::vmalloc::vmalloc_heal_stats();
            let heap = mm::heap::heap_stats();
            let kmalloc = mm::slub::kmalloc_stats();
            let layout = mm::snapshot();
            let boot_va_bits = mm::boot_reported_va_bits();
            let boot_levels = mm::boot_reported_page_levels();
            let hw_va_bits = mm::hardware_va_bits();
            let hw_levels = mm::hardware_page_levels();
            let boot_root = mm::boot_reported_root_phys();
            let hw_root = mm::hardware_root_phys();
            let corrected = mm::paging_corrected_by_hw();
            let root_mismatch = mm::paging_root_mismatch();
            kprintln!("Memory Status:");
            kprintln!("  Free pages:  {}", total_free);
            kprintln!(
                "  layout:      direct-map=[{:#x},{:#x}) vmalloc=[{:#x},{:#x}) va_bits={} levels={}",
                layout.direct_map_start,
                layout.direct_map_end,
                layout.vmalloc_start,
                layout.vmalloc_end,
                layout.va_bits,
                layout.page_levels
            );
            kprintln!(
                "  paging:      boot={}/L{} runtime={}/L{} hw={}/L{} corrected={}",
                boot_va_bits,
                boot_levels,
                layout.va_bits,
                layout.page_levels,
                hw_va_bits,
                hw_levels,
                corrected
            );
            kprintln!(
                "  roots:       boot={:#x} hw={:#x} mismatch={}",
                boot_root,
                hw_root,
                root_mismatch
            );
            kprintln!(
                "  kmalloc:     init={} active_caches={} objs={} (~{} KiB) slabs={} ({} KiB)",
                kmalloc.initialized,
                kmalloc.active_caches,
                kmalloc.total_allocated_objects,
                kmalloc.total_allocated_bytes / 1024,
                kmalloc.total_slabs,
                kmalloc.total_slab_bytes / 1024
            );
            kprintln!(
                "  kmalloc big: allocs={} pages={}",
                kmalloc.large_allocations,
                kmalloc.large_alloc_pages
            );
            kprintln!(
                "  heap(fallback): init={} total={} MiB used={} KiB free={} KiB segs={} live={}",
                heap.initialized,
                heap.total_size / 1024 / 1024,
                heap.used_size / 1024,
                heap.free_size / 1024,
                heap.segments,
                heap.live_allocations
            );
            kprintln!(
                "  vmalloc heal: ok={} miss={}",
                vmalloc_heal_ok,
                vmalloc_heal_miss
            );
            kprintln!(
                "  Faults:      total={} minor={} major={} cow={} stack_grow={}",
                fault_stats
                    .total_faults
                    .load(core::sync::atomic::Ordering::Relaxed),
                fault_stats
                    .minor_faults
                    .load(core::sync::atomic::Ordering::Relaxed),
                fault_stats
                    .major_faults
                    .load(core::sync::atomic::Ordering::Relaxed),
                fault_stats
                    .cow_faults
                    .load(core::sync::atomic::Ordering::Relaxed),
                fault_stats
                    .stack_grows
                    .load(core::sync::atomic::Ordering::Relaxed),
            );
        }
        "faults" => {
            let fault_stats = mm::get_fault_stats();
            kprintln!("Page Fault Stats:");
            kprintln!(
                "  total_faults: {}",
                fault_stats
                    .total_faults
                    .load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  minor_faults: {}",
                fault_stats
                    .minor_faults
                    .load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  major_faults: {}",
                fault_stats
                    .major_faults
                    .load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  cow_faults:   {}",
                fault_stats
                    .cow_faults
                    .load(core::sync::atomic::Ordering::Relaxed)
            );
            kprintln!(
                "  stack_grows:  {}",
                fault_stats
                    .stack_grows
                    .load(core::sync::atomic::Ordering::Relaxed)
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
                    kprintln!(
                        "  [{}] {:#x} - {:#x} ({} bytes)",
                        i,
                        region.base,
                        region.end(),
                        region.size
                    );
                }
            }

            kprintln!("Reserved Regions:");
            for i in 0..mm::memblock_reserved_region_count() {
                if let Some(region) = mm::memblock_reserved_region(i) {
                    kprintln!(
                        "  [{}] {:#x} - {:#x} ({} bytes)",
                        i,
                        region.base,
                        region.end(),
                        region.size
                    );
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

fn execute_drivers_command(args: &[&str]) {
    let subcommand = args.first().copied().unwrap_or("help");

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
                        kprintln!(
                            "  CPU #{}: APIC ID={}, Enabled={}, OnlineCapable={}",
                            cpu_count,
                            lapic.apic_id,
                            lapic.is_enabled(),
                            lapic.is_online_capable()
                        );
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
            kprintln!(
                "  Format:      {:?} ({})",
                if f == 0 {
                    "RGB"
                } else if f == 1 {
                    "BGR"
                } else {
                    "Other"
                },
                f
            );
        }
        "input" => {
            kprintln!("Input Devices Status:");
            kprintln!("  PS/2 Mouse ID: {:#x}", drivers::input::mouse_device_id());
            kprintln!("  PS/2 Keyboard: Initialized");

            // HID Status
            kprintln!(
                "  HID Keyboard:  {}",
                if drivers::input::hid::keyboard::is_present() {
                    "Present"
                } else {
                    "Not Present"
                }
            );
            let (k_head, k_tail) = drivers::input::hid::keyboard::buffer_status();
            kprintln!("    Buffer: {}/{} (Head/Tail)", k_head, k_tail);

            kprintln!(
                "  HID Mouse:     {}",
                if drivers::input::hid::mouse::is_present() {
                    "Present"
                } else {
                    "Not Present"
                }
            );
            let (m_head, m_tail) = drivers::input::hid::mouse::buffer_status();
            kprintln!("    Buffer: {}/{} (Head/Tail)", m_head, m_tail);

            let (h_head, h_tail) = drivers::input::hotkey_buffer_status();
            kprintln!(
                "  ACPI Hotkey:   {}",
                if drivers::input::has_hotkey_event() {
                    "Pending"
                } else {
                    "Empty"
                }
            );
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
            kprintln!(
                "Mouse Test Mode (ID: {:#x}) (Press any key to exit)",
                drivers::input::mouse_device_id()
            );
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
                    kprintln!(
                        "[{}] Mouse: X={:<4} Y={:<4} L={} M={} R={}",
                        current_count,
                        dx,
                        dy,
                        l,
                        m,
                        r
                    );
                }
                // 简单的防抖/延时
                for _ in 0..1000 {
                    core::hint::spin_loop();
                }
            }
            kprintln!("Exited mouse test mode.");
        }
        "interrupt" => {
            let int_enabled = interrupt::interrupts_enabled();
            kprintln!("Interrupt Status:");
            kprintln!(
                "  CPU Interrupts: {}",
                if int_enabled { "Enabled" } else { "Disabled" }
            );
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

fn execute_hotkey_command(args: &[&str]) {
    let subcommand = args.first().copied().unwrap_or("help");

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
            let name = args.get(1).copied().unwrap_or("");
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
        kprintln!(
            "  {:02x}:{:02x}.{} {:04x}:{:04x} Class {:02x} Sub {:02x} ProgIF {:02x}",
            addr.bus,
            addr.device,
            addr.function,
            header.vendor_id,
            header.device_id,
            header.class_code,
            header.subclass,
            header.prog_if
        );
    });
}

fn execute_usb_command() {
    drivers::usb::xhci::dump_devices();
}

fn execute_test_command(args: &[&str]) {
    let subcommand = args.first().copied().unwrap_or("help");

    match subcommand {
        "timer" => {
            info!("Testing timer (3 seconds)...");
            let start = interrupt::timer_ticks();
            while interrupt::timer_ticks() < start + 300 {
                interrupt::halt_with_interrupts();
            }
            crate::ok!(
                "Timer test passed: {} ticks",
                interrupt::timer_ticks() - start
            );
        }
        _ => {
            let routed = args.join(" ");
            crate::tests::run(routed.as_str());
        }
    }
}
