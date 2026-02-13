pub mod arch;

use crate::drivers::acpi::{Madt, MadtEntry};
use crate::interrupt;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::{info, ok, warn, kprint, kprintln};

static NEXT_CPU_ID: AtomicUsize = AtomicUsize::new(1); // BSP is 0

pub(crate) fn alloc_cpu_id() -> usize {
    NEXT_CPU_ID.fetch_add(1, Ordering::SeqCst)
}

/// 当前在线 CPU 数量（含 BSP）
pub fn cpu_count() -> usize {
    NEXT_CPU_ID.load(Ordering::SeqCst)
}

/// 自动检测并启动其他 CPU 核心
pub fn init(direct_map_base: u64, expected_cpus: usize) {
    if expected_cpus <= 1 {
        kprintln!("[diag][smp] skip smp init: expected_cpus={}", expected_cpus);
        return;
    }

    use crate::drivers::acpi;
    if let Some(madt) = acpi::find_table::<acpi::Madt>() {
        info!("SMP: Booting {} APs...", expected_cpus - 1);
        boot_aps(madt, direct_map_base, expected_cpus);
    }
}

/// 启动 AP 核心
///
/// # Arguments
/// * `madt` - MADT 表
/// * `direct_map_base` - 直接映射区基地址
/// * `expected_cpus` - 期望启动的 CPU 总数 (包括 BSP)
fn boot_aps(madt: &Madt, direct_map_base: u64, expected_cpus: usize) {
    let bsp_lapic_id = interrupt::local_apic_id();
    kprintln!(
        "[diag][smp] boot_aps begin bsp_apic_id={} expected_cpus={}",
        bsp_lapic_id,
        expected_cpus,
    );
    
    // 1. Prepare SMP environment (Trampoline or ACPI Wakeup)
    unsafe {
        arch::prepare_smp(madt, direct_map_base);
    }

    // 2. Iterate CPUs
    for entry in madt.entries() {
        if let MadtEntry::LocalApic(lapic) = entry {
            kprintln!(
                "[diag][smp] lapic entry apic_id={} enabled={} online_capable={}",
                lapic.apic_id,
                lapic.is_enabled(),
                lapic.is_online_capable(),
            );
            if lapic.apic_id as u32 == bsp_lapic_id {
                kprintln!("[diag][smp] skip bsp apic_id={}", lapic.apic_id);
                continue;
            }
            if !lapic.is_enabled() && !lapic.is_online_capable() {
                kprintln!("[diag][smp] skip disabled apic_id={}", lapic.apic_id);
                continue;
            }

            // Boot this AP
            let apic_id = lapic.apic_id;
            kprintln!("[diag][smp] booting apic_id={} ...", apic_id);
            arch::boot_ap(apic_id as u32, direct_map_base);
            kprintln!(
                "[diag][smp] boot_ap returned apic_id={} online_cpus_now={}",
                apic_id,
                NEXT_CPU_ID.load(Ordering::SeqCst),
            );
        }
    }
    
    // 3. Wait for APs to start
    let mut retries = 0;
    while NEXT_CPU_ID.load(Ordering::SeqCst) < expected_cpus {
        // Simple delay
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
        retries += 1;
        if retries > 100000 { // Timeout
            warn!("SMP: Timeout! Only {}/{} CPUs started.", 
                NEXT_CPU_ID.load(Ordering::SeqCst), expected_cpus);
            return;
        }
    }
    ok!("SMP: All {} CPUs active.", expected_cpus);
}
