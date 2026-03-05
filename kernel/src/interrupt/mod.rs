// ============================================================================
// january_os - 中断子系统
//
// 包含 GDT、IDT、异常处理和 APIC 支持
// ============================================================================
//!
//! # 初始化顺序
//!
//! ```text
//! 1. init_gdt()     - 设置 GDT 和 TSS
//! 2. init_idt()     - 设置 IDT 和异常处理程序
//! 3. init_apic()    - 初始化 Local APIC 和 I/O APIC
//! 4. init_timer()   - 初始化 APIC Timer
//! 5. enable_interrupts() - 启用中断
//! ```
//!
//! # 使用示例
//!
//! ```rust,ignore
//! // 内核初始化
//! interrupt::init(kernel_stack_top, &madt_info);
//!
//! // 后续启用中断
//! interrupt::enable_interrupts();
//! ```

pub mod arch;

/// APIC Timer 触发频率（Hz）
pub const TIMER_TICK_HZ: u64 = 100;

// Re-export common types and functions from arch
pub use arch::{InterruptInitInfo, IrqRouteOverride, init, init_ap, init_bsp, initialized};

// Re-export arch modules
pub use arch::{apic, gdt, handlers, idt, tsc};

// Re-export specific items for compatibility/convenience
pub use gdt::{
    KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, TSS_SELECTOR, USER_CODE_SELECTOR,
    USER_DATA_SELECTOR, init_gdt, set_interrupt_stack,
};

pub use idt::{
    ALIGNMENT_CHECK, BOUND_RANGE, BREAKPOINT, CONTROL_PROTECTION, DEBUG, DEVICE_NOT_AVAILABLE,
    DIVIDE_ERROR, DOUBLE_FAULT, GENERAL_PROTECTION, GateType, INVALID_OPCODE, INVALID_TSS,
    IPI_TLB_PROBE, IPI_TLB_SHOOTDOWN, IRQ_BASE, IRQ_COM1, IRQ_KEYBOARD, IRQ_MOUSE, IRQ_SPURIOUS,
    IRQ_TIMER, IRQ_XHCI, IdtEntry, InterruptFrame, MACHINE_CHECK, NMI, OVERFLOW, PAGE_FAULT,
    SEGMENT_NOT_PRESENT, SIMD_EXCEPTION, STACK_FAULT, VIRTUALIZATION, X87_FPU_ERROR,
    disable_interrupts, enable_interrupts, halt, halt_with_interrupts, interrupts_enabled,
    without_interrupts,
};

pub use apic::{
    ICR_DELIVERY_FIXED,
    ICR_DELIVERY_INIT,
    ICR_DELIVERY_LOWEST,
    ICR_DELIVERY_NMI,
    ICR_DELIVERY_SMI,
    ICR_DELIVERY_STARTUP,
    ICR_DEST_LOGICAL,
    ICR_DEST_PHYSICAL,
    ICR_LEVEL_ASSERT,
    ICR_LEVEL_DEASSERT,
    ICR_SHORTHAND_ALL,
    ICR_SHORTHAND_ALL_BUT_SELF,
    ICR_SHORTHAND_NONE,
    ICR_SHORTHAND_SELF,
    ICR_TRIGGER_EDGE,
    ICR_TRIGGER_LEVEL,
    IoApicIrqRoute,
    apic_initialized,
    apic_timer_frequency,
    calibrate_timer,
    init_apic_timer,
    init_ioapic,
    init_local_apic,
    ioapic_mask_irq,
    ioapic_read_irq_route,
    ioapic_set_irq,
    ioapic_unmask_irq,
    local_apic_eoi,
    local_apic_id,
    send_init_ipi,
    // IPI functions
    send_ipi,
    send_sipi,
    stop_apic_timer,
    wait_for_ipi_delivery,
};

pub use tsc::{calibrate_tsc, rdtsc, rdtscp, tsc_frequency};

pub use handlers::{set_timer_debug, timer_debug_heartbeats, timer_ticks};

// 从 drivers::input 重新导出键盘接口
pub use crate::drivers::input::{
    buffer_len, has_char, is_alt_pressed, is_ctrl_pressed, is_shift_pressed, last_char,
    last_scancode, read_char,
};
