//! x86_64 中断和异常处理。
//!
//! x86_64 特有的入口表、陷阱门、APIC 与定时器实现均在本目录内分域组织。

pub mod controller;
pub mod entry;
pub mod timer;
pub mod trap;

use crate::mm;
use controller::apic::{init_ioapic, init_local_apic, ioapic_set_irq, ioapic_unmask_irq};
use core::sync::atomic::{AtomicBool, Ordering};
use entry::gdt::{init_gdt, set_interrupt_stack};
use trap::idt::IRQ_KEYBOARD;
use trap::idt::{IRQ_COM1, IRQ_MOUSE};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
pub struct IrqRouteOverride {
    pub source: u8,
    pub gsi: u32,
    pub level_triggered: bool,
    pub active_low: bool,
}

#[derive(Debug, Clone)]
pub struct InterruptInitInfo {
    pub kernel_stack_top: u64,
    pub local_apic_addr: u64,
    pub ioapic_addr: u64,
    pub ioapic_gsi_base: u32,
    pub irq_override_count: usize,
    pub irq_overrides: [IrqRouteOverride; 16],
    pub direct_map_base: u64,
}

fn resolve_isa_irq_route(info: &InterruptInitInfo, isa_irq: u8) -> Option<(u8, bool, bool)> {
    for idx in 0..info.irq_override_count {
        let ov = info.irq_overrides[idx];
        if ov.source != isa_irq {
            continue;
        }

        if ov.gsi > u8::MAX as u32 {
            crate::warn!(
                "SMP/IOAPIC: ignore IRQ override source={} gsi={} (out of range)",
                isa_irq,
                ov.gsi
            );
            return None;
        }

        return Some((ov.gsi as u8, ov.level_triggered, ov.active_low));
    }

    Some((isa_irq, false, false))
}

fn setup_isa_irq(info: &InterruptInitInfo, isa_irq: u8, vector: u8, name: &str) {
    let Some((gsi, level_triggered, active_low)) = resolve_isa_irq_route(info, isa_irq) else {
        crate::warn!(
            "SMP/IOAPIC: skip {} route due to invalid override source={}",
            name,
            isa_irq
        );
        return;
    };

    ioapic_set_irq(gsi, vector, 0, level_triggered, active_low);
    ioapic_unmask_irq(gsi);
}

pub unsafe fn init_bsp(info: &InterruptInitInfo) -> Result<(), &'static str> {
    if INITIALIZED.load(Ordering::Relaxed) {
        return Err("Interrupt subsystem already initialized");
    }

    init_gdt(0, info.kernel_stack_top);
    crate::arch::syscall::init_syscall();

    if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
        let ist_top = info.direct_map_base + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
        set_interrupt_stack(0, 1, ist_top);
    }

    trap::idt::init()?;

    if info.local_apic_addr != 0 {
        init_local_apic(info.local_apic_addr);
    }

    if info.ioapic_addr != 0 {
        init_ioapic(info.ioapic_addr, info.ioapic_gsi_base);
        setup_isa_irq(info, 1, IRQ_KEYBOARD, "keyboard");
        setup_isa_irq(info, 12, IRQ_MOUSE, "mouse");
        setup_isa_irq(info, 4, IRQ_COM1, "serial");
    }

    INITIALIZED.store(true, Ordering::SeqCst);
    Ok(())
}

pub unsafe fn init(info: &InterruptInitInfo) -> Result<(), &'static str> {
    init_bsp(info)
}

pub unsafe fn init_ap(
    cpu_id: usize,
    kernel_stack_top: u64,
    local_apic_addr: u64,
    direct_map_base: u64,
) -> Result<(), &'static str> {
    crate::smp::ap_boot_probe_set_stage(31);

    init_gdt(cpu_id, kernel_stack_top);
    crate::smp::ap_boot_probe_set_stage(32);

    crate::arch::syscall::init_syscall();
    crate::smp::ap_boot_probe_set_stage(33);

    if local_apic_addr != 0 {
        init_local_apic(local_apic_addr);
    }
    crate::smp::ap_boot_probe_set_stage(34);

    if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
        let ist_top = direct_map_base + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
        set_interrupt_stack(cpu_id, 1, ist_top);
    }
    crate::smp::ap_boot_probe_set_stage(35);

    trap::idt::load_idt();
    crate::smp::ap_boot_probe_set_stage(36);
    crate::smp::ap_boot_probe_set_stage(37);
    Ok(())
}

pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}
