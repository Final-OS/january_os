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

pub mod gdt;
pub mod idt;
pub mod handlers;
pub mod apic;
pub mod pit;

// 键盘驱动已移至 drivers/input/ps2

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
    stop_apic_timer, send_ipi, send_ipi_all_excluding_self,
    ioapic_set_irq, ioapic_mask_irq, ioapic_unmask_irq,
    calibrate_timer, apic_timer_frequency,
    IpiDeliveryMode,
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

use core::sync::atomic::{AtomicBool, Ordering};

/// 中断子系统是否已初始化
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// 中断子系统初始化信息
#[derive(Debug, Clone)]
pub struct InterruptInitInfo {
    /// 内核栈顶地址
    pub kernel_stack_top: u64,
    /// Local APIC 物理地址
    pub local_apic_addr: u64,
    /// I/O APIC 物理地址
    pub ioapic_addr: u64,
    /// I/O APIC GSI 基址
    pub ioapic_gsi_base: u32,
}

/// 初始化中断子系统
///
/// # Arguments
/// * `info` - 初始化信息
///
/// # Safety
///
/// 必须在单核环境下调用，中断禁用
pub unsafe fn init(info: &InterruptInitInfo) -> Result<(), &'static str> {
    if INITIALIZED.load(Ordering::Relaxed) {
        return Err("Interrupt subsystem already initialized");
    }

    // 1. 初始化 GDT 和 TSS
    init_gdt(info.kernel_stack_top);

    // 2. 初始化 IDT
    init_idt()?;

    // 3. 初始化 Local APIC
    if info.local_apic_addr != 0 {
        init_local_apic(info.local_apic_addr);
    }

    // 4. 初始化 I/O APIC
    if info.ioapic_addr != 0 {
        init_ioapic(info.ioapic_addr, info.ioapic_gsi_base);
        
        // 设置键盘中断 (IRQ 1 -> vector 0x21)
        // 键盘是边沿触发、高电平有效
        ioapic_set_irq(1, IRQ_KEYBOARD, 0, false, false);
        ioapic_unmask_irq(1);

        // 设置鼠标中断 (IRQ 12 -> vector 0x2C)
        ioapic_set_irq(12, IRQ_MOUSE, 0, false, false);
        ioapic_unmask_irq(12);
        
        // 设置串口中断 (IRQ 4 -> vector 0x24)
        ioapic_set_irq(4, IRQ_COM1, 0, false, false);
        ioapic_unmask_irq(4);
    }

    INITIALIZED.store(true, Ordering::SeqCst);
    Ok(())
}

/// 初始化 IDT
unsafe fn init_idt() -> Result<(), &'static str> {
    let idt = idt::get_idt_mut();

    // 设置异常处理程序
    idt.set_handler(DIVIDE_ERROR, IdtEntry::trap(
        handlers::divide_error_handler as u64
    ));
    idt.set_handler(DEBUG, IdtEntry::trap(
        handlers::debug_handler as u64
    ));
    idt.set_handler(NMI, IdtEntry::interrupt(
        handlers::nmi_handler as u64
    ));
    idt.set_handler(BREAKPOINT, IdtEntry::trap(
        handlers::breakpoint_handler as u64
    ));
    idt.set_handler(OVERFLOW, IdtEntry::trap(
        handlers::overflow_handler as u64
    ));
    idt.set_handler(BOUND_RANGE, IdtEntry::trap(
        handlers::bound_range_handler as u64
    ));
    idt.set_handler(INVALID_OPCODE, IdtEntry::trap(
        handlers::invalid_opcode_handler as u64
    ));
    idt.set_handler(DEVICE_NOT_AVAILABLE, IdtEntry::trap(
        handlers::device_not_available_handler as u64
    ));
    // Double Fault 使用 IST 1 确保有可用栈
    idt.set_handler(DOUBLE_FAULT, IdtEntry::interrupt_ist(
        handlers::double_fault_handler as u64, 1
    ));
    idt.set_handler(INVALID_TSS, IdtEntry::trap(
        handlers::invalid_tss_handler as u64
    ));
    idt.set_handler(SEGMENT_NOT_PRESENT, IdtEntry::trap(
        handlers::segment_not_present_handler as u64
    ));
    idt.set_handler(STACK_FAULT, IdtEntry::trap(
        handlers::stack_fault_handler as u64
    ));
    idt.set_handler(GENERAL_PROTECTION, IdtEntry::trap(
        handlers::general_protection_handler as u64
    ));
    idt.set_handler(PAGE_FAULT, IdtEntry::trap(
        handlers::page_fault_handler as u64
    ));
    idt.set_handler(X87_FPU_ERROR, IdtEntry::trap(
        handlers::x87_fpu_error_handler as u64
    ));
    idt.set_handler(ALIGNMENT_CHECK, IdtEntry::trap(
        handlers::alignment_check_handler as u64
    ));
    idt.set_handler(MACHINE_CHECK, IdtEntry::interrupt(
        handlers::machine_check_handler as u64
    ));
    idt.set_handler(SIMD_EXCEPTION, IdtEntry::trap(
        handlers::simd_exception_handler as u64
    ));
    idt.set_handler(VIRTUALIZATION, IdtEntry::trap(
        handlers::virtualization_handler as u64
    ));
    idt.set_handler(CONTROL_PROTECTION, IdtEntry::trap(
        handlers::control_protection_handler as u64
    ));

    // 设置硬件中断处理程序
    idt.set_handler(IRQ_TIMER, IdtEntry::interrupt(
        handlers::timer_handler as u64
    ));
    idt.set_handler(IRQ_KEYBOARD, IdtEntry::interrupt(
        handlers::keyboard_handler as u64
    ));
    idt.set_handler(IRQ_MOUSE, IdtEntry::interrupt(
        handlers::mouse_handler as u64
    ));
    idt.set_handler(IRQ_COM1, IdtEntry::interrupt(
        handlers::serial_handler as u64
    ));
    idt.set_handler(IRQ_SPURIOUS, IdtEntry::interrupt(
        handlers::spurious_handler as u64
    ));

    // 加载 IDT
    idt::load_idt();

    Ok(())
}

/// 启动 APIC Timer
///
/// # Arguments
/// * `frequency_hz` - 中断频率 (Hz)
pub fn start_timer(frequency_hz: u32) {
    init_apic_timer(IRQ_TIMER, frequency_hz);
}

/// 检查中断子系统是否已初始化
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}
