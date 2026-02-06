pub mod arch;

use crate::drivers::acpi::{Madt, MadtEntry};
use crate::interrupt;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::{info, ok, warn, kprint, kprintln};

static NEXT_CPU_ID: AtomicUsize = AtomicUsize::new(1); // BSP is 0

pub(crate) fn alloc_cpu_id() -> usize {
    NEXT_CPU_ID.fetch_add(1, Ordering::SeqCst)
}

/// 自动检测并启动其他 CPU 核心
pub fn init(direct_map_base: u64, expected_cpus: usize) {
    if expected_cpus <= 1 {
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
    
    // 1. Prepare trampoline
    unsafe {
        arch::prepare_trampoline(direct_map_base);
    }

    // 2. Iterate CPUs
    for entry in madt.entries() {
        if let MadtEntry::LocalApic(lapic) = entry {
            if lapic.apic_id as u32 == bsp_lapic_id {
                continue;
            }
            if !lapic.is_enabled() && !lapic.is_online_capable() {
                continue;
            }

            // Boot this AP
            let apic_id = lapic.apic_id;
            arch::boot_ap(apic_id as u32, direct_map_base);
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
