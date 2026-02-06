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

// Re-export common types and functions from arch
pub use arch::{
    InterruptInitInfo,
    init,
    init_bsp,
    init_ap,
    initialized,
};

// Re-export arch modules
pub use arch::{gdt, idt, handlers, apic, pit};

// Re-export specific items for compatibility/convenience
pub use gdt::{
    KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR,
    USER_CODE_SELECTOR, USER_DATA_SELECTOR, TSS_SELECTOR,
    init_gdt, set_interrupt_stack,
};

pub use idt::{
    IdtEntry, GateType, InterruptFrame,
    enable_interrupts, disable_interrupts, interrupts_enabled,
    without_interrupts, halt, halt_with_interrupts,
    DIVIDE_ERROR, DEBUG, NMI, BREAKPOINT, OVERFLOW, BOUND_RANGE,
    INVALID_OPCODE, DEVICE_NOT_AVAILABLE, DOUBLE_FAULT,
    INVALID_TSS, SEGMENT_NOT_PRESENT, STACK_FAULT,
    GENERAL_PROTECTION, PAGE_FAULT, X87_FPU_ERROR,
    ALIGNMENT_CHECK, MACHINE_CHECK, SIMD_EXCEPTION,
    VIRTUALIZATION, CONTROL_PROTECTION,
    IRQ_BASE, IRQ_TIMER, IRQ_KEYBOARD, IRQ_MOUSE, IRQ_COM1, IRQ_SPURIOUS,
};

pub use apic::{
    init_local_apic, init_ioapic, init_apic_timer,
    local_apic_eoi, local_apic_id, apic_initialized,
    stop_apic_timer,
    ioapic_set_irq, ioapic_mask_irq, ioapic_unmask_irq,
    calibrate_timer, apic_timer_frequency,
    // IPI functions
    send_ipi, send_init_ipi, send_sipi, wait_for_ipi_delivery,
    ICR_DELIVERY_FIXED, ICR_DELIVERY_LOWEST, ICR_DELIVERY_SMI, ICR_DELIVERY_NMI,
    ICR_DELIVERY_INIT, ICR_DELIVERY_STARTUP,
    ICR_DEST_PHYSICAL, ICR_DEST_LOGICAL,
    ICR_LEVEL_DEASSERT, ICR_LEVEL_ASSERT,
    ICR_TRIGGER_EDGE, ICR_TRIGGER_LEVEL,
    ICR_SHORTHAND_NONE, ICR_SHORTHAND_SELF, ICR_SHORTHAND_ALL, ICR_SHORTHAND_ALL_BUT_SELF,
};

pub use pit::{
    pit_set_frequency, pit_get_ticks, pit_tick,
    PIT_FREQUENCY,
};

pub use handlers::{
    timer_ticks, set_timer_debug,
};

// 从 drivers::input 重新导出键盘接口
pub use crate::drivers::input::{
    read_char, has_char, buffer_len,
    last_scancode, last_char,
    is_shift_pressed, is_ctrl_pressed, is_alt_pressed,
};
