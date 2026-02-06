pub mod trampoline;

use crate::mm;
use crate::interrupt;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Read CR3 register
fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
    }
    cr3 & 0x000F_FFFF_FFFF_F000
}

/// Boot a specific AP
pub fn boot_ap(apic_id: u32, direct_map_base: u64) {
    let pml4_phys = read_cr3();
    
    // Allocate stack
    let stack_pages = 4; // 16KB
    let stack = mm::alloc_pages(stack_pages, mm::GFP_KERNEL).expect("Failed to alloc AP stack");
    let stack_top = direct_map_base + (mm::page_to_pfn(stack) as u64) * 4096 + (stack_pages as u64) * 4096;

    // Fill trampoline data
    unsafe {
        let data_base = (direct_map_base + trampoline::TRAMPOLINE_BASE + 4096) as *mut u8;
        *(data_base.sub(trampoline::OFFSET_ARG as usize) as *mut u64) = direct_map_base;
        *(data_base.sub(trampoline::OFFSET_CR3 as usize) as *mut u64) = pml4_phys;
        *(data_base.sub(trampoline::OFFSET_RSP as usize) as *mut u64) = stack_top;
        *(data_base.sub(trampoline::OFFSET_ENTRY as usize) as *mut u64) = ap_entry as u64;
    }
    
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
    
    let local_apic_base = 0xFEE00000; // Default address (TODO: Get from MSR or MADT?)
    
    unsafe {
        interrupt::init_ap(cpu_id, kernel_stack_top, local_apic_base, direct_map_base).unwrap();
    }
    
    crate::kprintln!("      [SMP] AP Started (CPU {})", cpu_id);
    
    loop {
        core::hint::spin_loop();
    }
}

/// Prepare trampoline code
pub unsafe fn prepare_trampoline(direct_map_base: u64) {
    let trampoline_target = direct_map_base + trampoline::TRAMPOLINE_BASE;
    let src = trampoline::TRAMPOLINE_CODE;
    core::ptr::copy_nonoverlapping(src.as_ptr(), trampoline_target as *mut u8, src.len());
}
