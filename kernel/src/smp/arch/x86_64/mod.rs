pub mod trampoline;

use crate::mm;
use crate::interrupt;
use crate::drivers::acpi::{Madt, MadtEntry, MultiprocessorWakeupMailbox};
use core::arch::global_asm;

// Global state for ACPI Wakeup (shared with ASM)
#[unsafe(no_mangle)]
static mut DIRECT_MAP_BASE_VAL: u64 = 0;
#[unsafe(no_mangle)]
static mut NEXT_AP_STACK_VAL: u64 = 0;

// Internal state
static mut MAILBOX_VIRT_ADDR: u64 = 0;

global_asm!(r#"
.section .text
.global acpi_wakeup_entry
.extern ap_entry
.extern DIRECT_MAP_BASE_VAL
.extern NEXT_AP_STACK_VAL

acpi_wakeup_entry:
    // Load stack pointer from NEXT_AP_STACK_VAL
    mov rax, [rip + NEXT_AP_STACK_VAL] 
    mov rsp, rax
    
    // Load direct_map_base into RDI (1st arg)
    mov rdi, [rip + DIRECT_MAP_BASE_VAL]
    
    // Jump to Rust entry
    jmp ap_entry
"#);

unsafe extern "C" {
    fn acpi_wakeup_entry();
}

/// Read CR3 register
fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
    }
    cr3 & 0x000F_FFFF_FFFF_F000
}

/// Prepare SMP environment
/// 
/// Checks for ACPI Multiprocessor Wakeup support.
/// If available, sets up the mailbox.
/// If not, prepares the Legacy Trampoline.
pub unsafe fn prepare_smp(madt: &Madt, direct_map_base: u64) {
    // Save direct_map_base for ASM
    DIRECT_MAP_BASE_VAL = direct_map_base;

    // 1. Check for ACPI Multiprocessor Wakeup Mailbox
    let mut mp_wakeup = None;
    for entry in madt.entries() {
        if let MadtEntry::MultiprocessorWakeup(w) = entry {
            mp_wakeup = Some(w);
            break;
        }
    }

    if let Some(wakeup) = mp_wakeup {
        let addr = wakeup.mailbox_address;
        crate::info!("SMP: Using ACPI Multiprocessor Wakeup (Mailbox: {:#x})", addr);
        MAILBOX_VIRT_ADDR = direct_map_base + addr;
    } else {
        crate::info!("SMP: Using Legacy Trampoline");
        prepare_trampoline(direct_map_base);
    }
}

/// Boot a specific AP
pub fn boot_ap(apic_id: u32, direct_map_base: u64) {
    // Allocate stack
    let stack_pages = 4; // 16KB
    let stack = mm::alloc_pages(stack_pages, mm::GFP_KERNEL).expect("Failed to alloc AP stack");
    let stack_top = direct_map_base + (mm::page_to_pfn(stack) as u64) * 4096 + (stack_pages as u64) * 4096;

    // Save stack for ACPI Wakeup (if used)
    unsafe {
        NEXT_AP_STACK_VAL = stack_top;
    }

    unsafe {
        if MAILBOX_VIRT_ADDR != 0 {
            boot_ap_acpi(apic_id, stack_top);
        } else {
            boot_ap_legacy(apic_id, direct_map_base, stack_top);
        }
    }
}

/// Boot AP using ACPI Multiprocessor Wakeup
unsafe fn boot_ap_acpi(apic_id: u32, _stack_top: u64) {
    crate::info!("  [SMP] Booting AP {} via ACPI Wakeup...", apic_id);
    let mailbox = &mut *(MAILBOX_VIRT_ADDR as *mut MultiprocessorWakeupMailbox);
    
    // 1. Setup Mailbox
    // Use addr_of_mut! to avoid unaligned references
    core::ptr::write_volatile(core::ptr::addr_of_mut!(mailbox.apic_id), apic_id);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(mailbox.wakeup_vector), acpi_wakeup_entry as u64);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(mailbox.command), MultiprocessorWakeupMailbox::COMMAND_WAKEUP);

    // 2. Wait for wakeup (optional, but good for debugging)
    // The AP should clear the command bit or we can just rely on SMP timeout in main loop
}

/// Boot AP using Legacy Trampoline (SIPI)
unsafe fn boot_ap_legacy(apic_id: u32, direct_map_base: u64, stack_top: u64) {
    crate::info!("  [SMP] Booting AP {} via Legacy Trampoline...", apic_id);
    let pml4_phys = read_cr3();
    
    // Fill trampoline data
    let data_base = (direct_map_base + trampoline::TRAMPOLINE_BASE + 4096) as *mut u8;
    *(data_base.sub(trampoline::OFFSET_ARG as usize) as *mut u64) = direct_map_base;
    *(data_base.sub(trampoline::OFFSET_CR3 as usize) as *mut u64) = pml4_phys;
    *(data_base.sub(trampoline::OFFSET_RSP as usize) as *mut u64) = stack_top;
    *(data_base.sub(trampoline::OFFSET_ENTRY as usize) as *mut u64) = ap_entry as u64;
    
    // Send INIT-SIPI-SIPI
    interrupt::send_init_ipi(apic_id);
    delay_ms(10);
    
    let vector = (trampoline::TRAMPOLINE_BASE >> 12) as u8; // 0x08
    interrupt::send_sipi(apic_id, vector);
    delay_us(200);
    interrupt::send_sipi(apic_id, vector);
}

fn delay_ms(ms: u64) {
    for _ in 0..ms * 10000 {
        core::hint::spin_loop();
    }
}
fn delay_us(us: u64) {
    for _ in 0..us * 10 {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_entry(direct_map_base: u64) -> ! {
    let cpu_id = crate::smp::alloc_cpu_id();
    
    // Init AP interrupts
    // We need kernel_stack_top. It is current RSP.
    let kernel_stack_top: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) kernel_stack_top);
    }
    
    let local_apic_base = 0xFEE00000; // Default address
    
    unsafe {
        interrupt::init_ap(cpu_id, kernel_stack_top, local_apic_base, direct_map_base).unwrap();
    }
    
    crate::kprintln!("      [SMP] AP Started (CPU {})", cpu_id);
    
    loop {
        core::hint::spin_loop();
    }
}

/// Prepare trampoline code (Legacy)
unsafe fn prepare_trampoline(direct_map_base: u64) {
    let trampoline_target = direct_map_base + trampoline::TRAMPOLINE_BASE;
    let src = trampoline::TRAMPOLINE_CODE;
    core::ptr::copy_nonoverlapping(src.as_ptr(), trampoline_target as *mut u8, src.len());
}
