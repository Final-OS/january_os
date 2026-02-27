pub mod arch;

use crate::drivers::acpi::{Madt, MadtEntry};
use crate::interrupt;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use crate::{info, ok, warn, kprint, kprintln};

/// CPU ID 分配器（BSP 固定为 0，AP 从 1 开始）
static NEXT_CPU_ID: AtomicUsize = AtomicUsize::new(1);
/// 检测到的 CPU 总数（来自 ACPI/平台枚举，含 BSP）
static DETECTED_CPU_COUNT: AtomicUsize = AtomicUsize::new(1);
/// 真正在线并完成初始化的 CPU 数（初始仅 BSP）
static ONLINE_CPU_COUNT: AtomicUsize = AtomicUsize::new(1);
/// AP 启动阶段诊断（单次串行启动 AP，故全局单槽足够）
static AP_BOOT_PROBE_CPU_ID: AtomicUsize = AtomicUsize::new(usize::MAX);
static AP_BOOT_PROBE_STAGE: AtomicU32 = AtomicU32::new(0);

pub(crate) fn alloc_cpu_id() -> usize {
    NEXT_CPU_ID.fetch_add(1, Ordering::SeqCst)
}

pub(crate) fn mark_cpu_online() -> usize {
    ONLINE_CPU_COUNT.fetch_add(1, Ordering::SeqCst) + 1
}

pub(crate) fn ap_boot_probe_reset() {
    AP_BOOT_PROBE_CPU_ID.store(usize::MAX, Ordering::SeqCst);
    AP_BOOT_PROBE_STAGE.store(0, Ordering::SeqCst);
}

pub(crate) fn ap_boot_probe_set_cpu(cpu_id: usize) {
    AP_BOOT_PROBE_CPU_ID.store(cpu_id, Ordering::SeqCst);
}

pub(crate) fn ap_boot_probe_set_stage(stage: u32) {
    AP_BOOT_PROBE_STAGE.store(stage, Ordering::SeqCst);
}

pub(crate) fn ap_boot_probe_snapshot() -> (usize, u32) {
    (
        AP_BOOT_PROBE_CPU_ID.load(Ordering::SeqCst),
        AP_BOOT_PROBE_STAGE.load(Ordering::SeqCst),
    )
}

/// 当前在线 CPU 数量（含 BSP）
pub fn cpu_count() -> usize {
    ONLINE_CPU_COUNT.load(Ordering::SeqCst)
}

/// 平台检测到的 CPU 数量（含 BSP）
pub fn detected_cpu_count() -> usize {
    DETECTED_CPU_COUNT.load(Ordering::SeqCst)
}

/// 自动检测并启动其他 CPU 核心
pub fn init(direct_map_base: u64, expected_cpus: usize) {
    DETECTED_CPU_COUNT.store(expected_cpus.max(1), Ordering::SeqCst);

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
    let mut target_online_cpus = 1usize; // BSP
    let mut launch_aborted = false;
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
            if arch::boot_ap(apic_id as u32, direct_map_base) {
                target_online_cpus += 1;
            } else {
                warn!(
                    "SMP: abort booting remaining APs after boot timeout/failure (failed_apic_id={})",
                    apic_id
                );
                launch_aborted = true;
                break;
            }
            kprintln!(
                "[diag][smp] boot_ap returned apic_id={} online_cpus_now={}",
                apic_id,
                cpu_count(),
            );
        }
    }
    
    // 3. Wait for APs to become fully online
    let mut retries = 0;
    while cpu_count() < target_online_cpus {
        // Simple delay
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
        retries += 1;
        if retries > 100000 { // Timeout
            warn!("SMP: Timeout! Only {}/{} CPUs became online.", cpu_count(), target_online_cpus);
            return;
        }
    }
    if launch_aborted {
        warn!(
            "SMP: AP launch aborted; online CPUs={}/{} (detected={})",
            cpu_count(),
            target_online_cpus,
            expected_cpus,
        );
    } else {
        ok!("SMP: All {} CPUs active.", target_online_cpus);
    }
}
