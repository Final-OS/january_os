// ============================================================================
// january_os - 中断子系统 façade
//
// 提供架构无关的生命周期、通用中断控制与诊断入口；x86_64 特有的
// GDT/IDT/APIC/TSC 组织在 `interrupt::arch::x86_64`。
// ============================================================================

pub mod arch;
pub mod controller;
pub mod core;
pub mod diag;
pub mod runtime;
pub mod trap;

pub use arch::{init, init_ap, init_bsp, initialized};
pub use diag::{dump_state, stats, timer_ticks};
pub use runtime::{init_core, init_early, init_late};

#[cfg(target_arch = "x86_64")]
pub use arch::x86_64::controller::apic::{
    apic_initialized, apic_timer_frequency, calibrate_timer, init_apic_timer, init_ioapic,
    init_local_apic, ioapic_mask_irq, ioapic_read_irq_route, ioapic_set_irq, ioapic_unmask_irq,
    local_apic_eoi, local_apic_id, send_init_ipi, send_ipi, send_sipi, stop_apic_timer,
    wait_for_ipi_delivery, IoApicIrqRoute, ICR_DELIVERY_FIXED, ICR_DELIVERY_INIT,
    ICR_DELIVERY_LOWEST, ICR_DELIVERY_NMI, ICR_DELIVERY_SMI, ICR_DELIVERY_STARTUP,
    ICR_DEST_LOGICAL, ICR_DEST_PHYSICAL, ICR_LEVEL_ASSERT, ICR_LEVEL_DEASSERT, ICR_SHORTHAND_ALL,
    ICR_SHORTHAND_ALL_BUT_SELF, ICR_SHORTHAND_NONE, ICR_SHORTHAND_SELF, ICR_TRIGGER_EDGE,
    ICR_TRIGGER_LEVEL,
};

#[cfg(target_arch = "x86_64")]
pub use arch::x86_64::timer::pit::{pit_get_ticks, pit_set_frequency, pit_tick};

#[cfg(target_arch = "x86_64")]
pub use arch::x86_64::timer::tsc::{calibrate_tsc, rdtsc, rdtscp, tsc_frequency};

#[cfg(target_arch = "x86_64")]
pub use arch::x86_64::trap::idt::{
    disable_interrupts, enable_interrupts, halt, halt_with_interrupts, interrupts_enabled,
    without_interrupts, GateType, IdtEntry, InterruptFrame, ALIGNMENT_CHECK, BOUND_RANGE,
    BREAKPOINT, CONTROL_PROTECTION, DEBUG, DEVICE_NOT_AVAILABLE, DIVIDE_ERROR, DOUBLE_FAULT,
    GENERAL_PROTECTION, INVALID_OPCODE, INVALID_TSS, IPI_TLB_PROBE, IPI_TLB_SHOOTDOWN, IRQ_BASE,
    IRQ_COM1, IRQ_KEYBOARD, IRQ_MOUSE, IRQ_SPURIOUS, IRQ_TIMER, IRQ_XHCI, MACHINE_CHECK, NMI,
    OVERFLOW, PAGE_FAULT, SEGMENT_NOT_PRESENT, SIMD_EXCEPTION, STACK_FAULT, VIRTUALIZATION,
    X87_FPU_ERROR,
};

pub use crate::drivers::input::{
    buffer_len, has_char, is_alt_pressed, is_ctrl_pressed, is_shift_pressed, last_char,
    last_scancode, read_char,
};

use crate::component::{ComponentDescriptor, ComponentStage};

/// APIC Timer 触发频率（Hz）
pub const TIMER_TICK_HZ: u64 = 100;

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "interrupt",
    stage: ComponentStage::Core,
    deps: &["memory", "acpi"],
    summary: "interrupt lifecycle, trap control and arch-specific routing façade",
};
