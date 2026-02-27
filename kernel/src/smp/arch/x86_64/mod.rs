pub mod trampoline;

use crate::mm;
use crate::interrupt;
use crate::drivers::acpi::{Madt, MadtEntry, MultiprocessorWakeupMailbox};
use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Global state for ACPI Wakeup (shared with ASM)
#[unsafe(no_mangle)]
static DIRECT_MAP_BASE_VAL: AtomicU64 = AtomicU64::new(0);
#[unsafe(no_mangle)]
static NEXT_AP_STACK_VAL: AtomicU64 = AtomicU64::new(0);

// Internal state
static MAILBOX_VIRT_ADDR: AtomicU64 = AtomicU64::new(0);

/// AP signals it has started and read trampoline data
static AP_STARTED: AtomicBool = AtomicBool::new(false);
const AP_START_TIMEOUT_SPINS: u64 = 10_000_000;

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
    DIRECT_MAP_BASE_VAL.store(direct_map_base, Ordering::Release);

    // 始终准备 Legacy Trampoline，便于 ACPI wakeup 失败时回退。
    prepare_trampoline(direct_map_base);

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
        crate::info!(
            "SMP: Using ACPI Multiprocessor Wakeup (Mailbox: {:#x}, legacy fallback ready)",
            addr
        );
        MAILBOX_VIRT_ADDR.store(direct_map_base + addr, Ordering::Release);
    } else {
        MAILBOX_VIRT_ADDR.store(0, Ordering::Release);
        crate::info!("SMP: Using Legacy Trampoline");
    }
}

/// Boot a specific AP
pub fn boot_ap(apic_id: u32, direct_map_base: u64) -> bool {
    // Allocate stack: order=2 → 2^2 = 4 pages = 16KB
    let stack_order = 2;
    let stack_pages = 1u64 << stack_order; // 4
    let stack = mm::alloc_pages(stack_order, mm::GFP_KERNEL).expect("Failed to alloc AP stack");
    let stack_top = direct_map_base + (mm::page_to_pfn(stack) as u64) * 4096 + stack_pages * 4096;

    crate::kprintln!(
        "[diag][smp] apic_id={} ap_stack_top={:#x} ap_stack_pfn={:#x}",
        apic_id,
        stack_top,
        mm::page_to_pfn(stack),
    );

    // Save stack for ACPI Wakeup (if used)
    NEXT_AP_STACK_VAL.store(stack_top, Ordering::Release);
    AP_STARTED.store(false, Ordering::Release);
    crate::smp::ap_boot_probe_reset();
    let online_before = crate::smp::cpu_count();

    let mailbox_virt = MAILBOX_VIRT_ADDR.load(Ordering::Acquire);
    let started = unsafe {
        if mailbox_virt != 0 {
            if boot_ap_acpi(apic_id, stack_top) {
                true
            } else {
                crate::warn!(
                    "  [SMP] AP {} ACPI wakeup failed, fallback to Legacy Trampoline",
                    apic_id
                );
                AP_STARTED.store(false, Ordering::Release);
                boot_ap_legacy(apic_id, direct_map_base, stack_top)
            }
        } else {
            boot_ap_legacy(apic_id, direct_map_base, stack_top)
        }
    };

    if !started {
        return false;
    }

    let mut timeout = 0u64;
    while crate::smp::cpu_count() <= online_before {
        core::hint::spin_loop();
        timeout += 1;
        if timeout > AP_START_TIMEOUT_SPINS {
            let (probe_cpu_id, probe_stage) = crate::smp::ap_boot_probe_snapshot();
            crate::warn!(
                "  [SMP] AP {} started but did not reach online state (online_before={} online_now={} probe_cpu_id={} probe_stage={})",
                apic_id,
                online_before,
                crate::smp::cpu_count(),
                probe_cpu_id,
                probe_stage,
            );
            return false;
        }
    }

    true
}

/// Boot AP using ACPI Multiprocessor Wakeup
unsafe fn boot_ap_acpi(apic_id: u32, _stack_top: u64) -> bool {
    crate::info!("  [SMP] Booting AP {} via ACPI Wakeup...", apic_id);
    let mailbox_virt = MAILBOX_VIRT_ADDR.load(Ordering::Acquire);
    let mailbox = &mut *(mailbox_virt as *mut MultiprocessorWakeupMailbox);

    // 1. Setup Mailbox
    core::ptr::write_volatile(core::ptr::addr_of_mut!(mailbox.apic_id), apic_id);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(mailbox.wakeup_vector), acpi_wakeup_entry as *const () as u64);
    // 内存屏障确保 apic_id 和 wakeup_vector 在 command 之前可见
    core::sync::atomic::fence(Ordering::Release);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(mailbox.command), MultiprocessorWakeupMailbox::COMMAND_WAKEUP);
    crate::kprintln!("[diag][smp] acpi wakeup sent apic_id={}", apic_id);

    // 2. 等待 AP 启动并读取完数据
    let mut timeout = 0u64;
    while !AP_STARTED.load(Ordering::Acquire) {
        core::hint::spin_loop();
        timeout += 1;
        if timeout > AP_START_TIMEOUT_SPINS {
            crate::warn!("  [SMP] AP {} (ACPI) start timeout!", apic_id);
            AP_STARTED.store(false, Ordering::Release);
            return false;
        }
    }
    AP_STARTED.store(false, Ordering::Release);
    true
}

/// Boot AP using Legacy Trampoline (SIPI)
unsafe fn boot_ap_legacy(apic_id: u32, direct_map_base: u64, stack_top: u64) -> bool {
    crate::info!("  [SMP] Booting AP {} via Legacy Trampoline...", apic_id);
    let pml4_phys = read_cr3();

    // Read BSP's GDTR and IDTR
    let mut gdtr: [u8; 10] = [0; 10];
    let mut idtr: [u8; 10] = [0; 10];
    core::arch::asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr(), options(nostack, preserves_flags));
    core::arch::asm!("sidt [{}]", in(reg) idtr.as_mut_ptr(), options(nostack, preserves_flags));

    // Fill trampoline data
    let data_base = (direct_map_base + trampoline::TRAMPOLINE_BASE + 4096) as *mut u8;
    *(data_base.sub(trampoline::OFFSET_ARG as usize) as *mut u64) = direct_map_base;
    *(data_base.sub(trampoline::OFFSET_CR3 as usize) as *mut u64) = pml4_phys;
    *(data_base.sub(trampoline::OFFSET_RSP as usize) as *mut u64) = stack_top;
    *(data_base.sub(trampoline::OFFSET_ENTRY as usize) as *mut u64) = ap_entry as *const () as u64;
    // Copy GDTR and IDTR (10 bytes each)
    core::ptr::copy_nonoverlapping(gdtr.as_ptr(), data_base.sub(trampoline::OFFSET_GDTR as usize), 10);
    core::ptr::copy_nonoverlapping(idtr.as_ptr(), data_base.sub(trampoline::OFFSET_IDTR as usize), 10);
    
    // Send INIT-SIPI-SIPI
    interrupt::send_init_ipi(apic_id);
    crate::kprintln!("[diag][smp] INIT sent apic_id={}", apic_id);
    delay_ms(10);
    
    let vector = (trampoline::TRAMPOLINE_BASE >> 12) as u8; // 0x08
    crate::kprintln!("[diag][smp] SIPI vector={:#x} apic_id={}", vector, apic_id);
    interrupt::send_sipi(apic_id, vector);
    delay_us(200);
    interrupt::send_sipi(apic_id, vector);

    // Wait for AP to signal it has started (and read trampoline data)
    let mut timeout = 0u64;
    while !AP_STARTED.load(Ordering::Acquire) {
        core::hint::spin_loop();
        timeout += 1;
        if timeout > AP_START_TIMEOUT_SPINS {
            crate::warn!("  [SMP] AP {} start timeout!", apic_id);
            // 超时也必须重置标志，但不能继续启动下一个 AP
            // 因为当前 AP 可能稍后才读取 trampoline 数据
            AP_STARTED.store(false, Ordering::Release);
            return false;
        }
    }
    AP_STARTED.store(false, Ordering::Release);
    true
}

fn delay_ms(ms: u64) {
    let freq = crate::interrupt::tsc_frequency();
    if freq > 0 {
        let start = crate::interrupt::rdtsc();
        let wait = freq * ms / 1000;
        while crate::interrupt::rdtsc() - start < wait {
            core::hint::spin_loop();
        }
    } else {
        // TSC 未校准时的回退
        for _ in 0..ms * 10000 {
            core::hint::spin_loop();
        }
    }
}
fn delay_us(us: u64) {
    let freq = crate::interrupt::tsc_frequency();
    if freq > 0 {
        let start = crate::interrupt::rdtsc();
        let wait = freq * us / 1_000_000;
        while crate::interrupt::rdtsc() - start < wait {
            core::hint::spin_loop();
        }
    } else {
        for _ in 0..us * 10 {
            core::hint::spin_loop();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_entry(direct_map_base: u64) -> ! {
    // Signal BSP that we have started and read trampoline data
    AP_STARTED.store(true, Ordering::Release);
    let _ = direct_map_base;
    crate::smp::ap_boot_probe_set_stage(1);

    let cpu_id = crate::smp::alloc_cpu_id();
    crate::smp::ap_boot_probe_set_cpu(cpu_id);
    crate::smp::ap_boot_probe_set_stage(2);
    
    // Init AP interrupts
    // We need kernel_stack_top. It is current RSP.
    let kernel_stack_top: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) kernel_stack_top);
    }
    
    let local_apic_base = 0xFEE00000; // Default address
    crate::smp::ap_boot_probe_set_stage(3);
    
    unsafe {
        interrupt::init_ap(cpu_id, kernel_stack_top, local_apic_base, direct_map_base).unwrap();
    }
    crate::smp::ap_boot_probe_set_stage(4);
    interrupt::enable_interrupts();
    crate::smp::ap_boot_probe_set_stage(5);
    crate::mm::paging::register_tlb_shootdown_cpu();
    crate::smp::ap_boot_probe_set_stage(6);
    let online_cpus = crate::smp::mark_cpu_online();
    crate::smp::ap_boot_probe_set_stage(7);

    crate::kprintln!(
        "[diag][smp] ap_entry cpu_id={} local_apic_id={} if={} online_cpus={}",
        cpu_id,
        interrupt::local_apic_id(),
        interrupt::interrupts_enabled(),
        online_cpus,
    );
    
    crate::kprintln!("      [SMP] AP Started (CPU {})", cpu_id);

    crate::task::scheduler::run_idle()
}

/// Prepare trampoline code (Legacy)
unsafe fn prepare_trampoline(direct_map_base: u64) {
    let trampoline_target = direct_map_base + trampoline::TRAMPOLINE_BASE;
    let src = trampoline::TRAMPOLINE_CODE;
    core::ptr::copy_nonoverlapping(src.as_ptr(), trampoline_target as *mut u8, src.len());
}
