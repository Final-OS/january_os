// ============================================================================
// january_os - 中断处理程序
//
// 异常和中断的具体处理逻辑
// ============================================================================

use super::idt::{InterruptFrame, InterruptFrameWithError};
use core::arch::asm;
use crate::mm::fault::{FaultContext, FaultResult, handle_page_fault};

// ============================================================================
// 异常处理程序
// ============================================================================

/// 除法错误处理程序 (#DE)
pub extern "x86-interrupt" fn divide_error_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: Divide Error (#DE)\n{:#?}", frame);
}

/// 调试异常处理程序 (#DB)
pub extern "x86-interrupt" fn debug_handler(frame: InterruptFrame) {
    // 调试异常可以继续执行
    let _ = frame;
}

/// NMI 处理程序
pub extern "x86-interrupt" fn nmi_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: Non-Maskable Interrupt (NMI)\n{:#?}", frame);
}

/// 断点处理程序 (#BP)
pub extern "x86-interrupt" fn breakpoint_handler(frame: InterruptFrame) {
    // 断点可以用于调试，打印信息后继续
    let _ = frame;
}

/// 溢出处理程序 (#OF)
pub extern "x86-interrupt" fn overflow_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: Overflow (#OF)\n{:#?}", frame);
}

/// 边界检查处理程序 (#BR)
pub extern "x86-interrupt" fn bound_range_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: BOUND Range Exceeded (#BR)\n{:#?}", frame);
}

/// 无效操作码处理程序 (#UD)
pub extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: Invalid Opcode (#UD)\n{:#?}", frame);
}

/// 设备不可用处理程序 (#NM)
pub extern "x86-interrupt" fn device_not_available_handler(frame: InterruptFrame) {
    // 通常用于延迟 FPU 状态切换
    panic!("EXCEPTION: Device Not Available (#NM)\n{:#?}", frame);
}

/// 双重故障处理程序 (#DF)
/// 
/// 使用 IST 确保有可用的栈
pub extern "x86-interrupt" fn double_fault_handler(
    frame: InterruptFrame,
    error_code: u64,
) -> ! {
    panic!(
        "EXCEPTION: Double Fault (#DF)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

/// 无效 TSS 处理程序 (#TS)
pub extern "x86-interrupt" fn invalid_tss_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: Invalid TSS (#TS)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

/// 段不存在处理程序 (#NP)
pub extern "x86-interrupt" fn segment_not_present_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: Segment Not Present (#NP)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

/// 栈故障处理程序 (#SS)
pub extern "x86-interrupt" fn stack_fault_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: Stack Fault (#SS)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

/// 通用保护故障处理程序 (#GP)
pub extern "x86-interrupt" fn general_protection_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: General Protection Fault (#GP)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

/// 页错误处理程序 (#PF)
pub extern "x86-interrupt" fn page_fault_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    // 获取触发页错误的地址 (CR2)
    let fault_addr: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) fault_addr, options(nostack, preserves_flags));
    }

    // 调用 mm 模块的页错误处理。
    // 当前尚未引入 per-process mm，上下文先绑定到 init_mm，避免空指针路径。
    let direct_map = crate::config::DIRECT_MAP_OFFSET;
    let mut init_mm = crate::mm::get_init_mm();
    let mm_ptr: *mut crate::mm::Mm = &mut *init_mm;
    let mut ctx = FaultContext::new(fault_addr, error_code, mm_ptr, direct_map);
    let result = handle_page_fault(&mut ctx);

    match result {
        FaultResult::Retry => {
            // 页错误已处理，CPU 将重试指令
            return;
        }
        _ => {
            // 无法处理的页错误
            let present = error_code & 0x1 != 0;
            let write = error_code & 0x2 != 0;
            let user = error_code & 0x4 != 0;
            let reserved = error_code & 0x8 != 0;
            let instruction_fetch = error_code & 0x10 != 0;

            panic!(
                "EXCEPTION: Page Fault (#PF)\n\
                 Fault address: {:#x}\n\
                 Error code: {:#x}\n\
                 Result: {:?}\n\
                 - Page present: {}\n\
                 - Write access: {}\n\
                 - User mode: {}\n\
                 - Reserved bit: {}\n\
                 - Instruction fetch: {}\n\
                 {:#?}",
                fault_addr, error_code, result,
                present, write, user, reserved, instruction_fetch,
                frame
            );
        }
    }
}

/// x87 FPU 错误处理程序 (#MF)
pub extern "x86-interrupt" fn x87_fpu_error_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: x87 FPU Error (#MF)\n{:#?}", frame);
}

/// 对齐检查处理程序 (#AC)
pub extern "x86-interrupt" fn alignment_check_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: Alignment Check (#AC)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

/// 机器检查处理程序 (#MC)
pub extern "x86-interrupt" fn machine_check_handler(frame: InterruptFrame) -> ! {
    panic!("EXCEPTION: Machine Check (#MC)\n{:#?}", frame);
}

/// SIMD 异常处理程序 (#XM)
pub extern "x86-interrupt" fn simd_exception_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: SIMD Exception (#XM)\n{:#?}", frame);
}

/// 虚拟化异常处理程序 (#VE)
pub extern "x86-interrupt" fn virtualization_handler(frame: InterruptFrame) {
    panic!("EXCEPTION: Virtualization Exception (#VE)\n{:#?}", frame);
}

/// 控制保护异常处理程序 (#CP)
pub extern "x86-interrupt" fn control_protection_handler(
    frame: InterruptFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: Control Protection (#CP)\nError code: {:#x}\n{:#?}",
        error_code, frame
    );
}

// ============================================================================
// 硬件中断处理程序
// ============================================================================

use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};

/// Timer tick 计数
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// 是否启用 Timer 调试输出
static TIMER_DEBUG: AtomicBool = AtomicBool::new(false);

/// 获取 Timer tick 计数
pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

/// 启用/禁用 Timer 调试输出
pub fn set_timer_debug(enable: bool) {
    TIMER_DEBUG.store(enable, Ordering::Relaxed);
}

/// Timer 中断处理程序
pub extern "x86-interrupt" fn timer_handler(frame: InterruptFrame) {
    let ticks = TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    let _ = frame;
    
    // 每秒打印一次 (假设 100Hz)
    #[cfg(debug_assertions)]
    if TIMER_DEBUG.load(Ordering::Relaxed) && ticks % 100 == 0 {
        // 简单的串口输出 (避免在中断中使用复杂的 kprintln)
        unsafe {
            let seconds = ticks / 100;
            // 输出 '.' 表示活动
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3F8u16,
                in("al") b'.',
                options(nostack, preserves_flags)
            );
        }
    }
    
    // 发送 EOI
    super::apic::local_apic_eoi();
}

/// Keyboard 中断处理程序
pub extern "x86-interrupt" fn keyboard_handler(frame: InterruptFrame) {
    // 读取扫描码
    let scancode: u8;
    unsafe {
        asm!(
            "in al, 0x60",
            out("al") scancode,
            options(nostack, preserves_flags)
        );
    }
    
    let _ = frame;
    
    // 处理扫描码
    crate::drivers::input::handle_scancode(scancode);
    
    // 发送 EOI
    super::apic::local_apic_eoi();
}

/// Mouse 中断处理程序
pub extern "x86-interrupt" fn mouse_handler(frame: InterruptFrame) {
    // 读取数据
    let data: u8;
    unsafe {
        asm!(
            "in al, 0x60",
            out("al") data,
            options(nostack, preserves_flags)
        );
    }
    
    let _ = frame;
    
    // 处理鼠标数据
    crate::drivers::input::mouse_handle_interrupt(data);
    
    // 发送 EOI
    super::apic::local_apic_eoi();
}

/// 串口 (COM1) 中断处理程序
pub extern "x86-interrupt" fn serial_handler(frame: InterruptFrame) {
    let _ = frame;
    
    // 调用串口驱动处理
    crate::drivers::tty::serial_interrupt_handler();
    
    // 发送 EOI
    super::apic::local_apic_eoi();
}

/// xHCI 中断处理程序
pub extern "x86-interrupt" fn xhci_handler(frame: InterruptFrame) {
    let _ = frame;
    
    // 调用 xHCI 驱动处理
    crate::drivers::usb::xhci::handle_interrupt();
    
    // 发送 EOI
    super::apic::local_apic_eoi();
}

/// TLB shootdown IPI 处理程序
pub extern "x86-interrupt" fn tlb_shootdown_handler(frame: InterruptFrame) {
    let _ = frame;

    crate::mm::paging::handle_tlb_shootdown_ipi();

    super::apic::local_apic_eoi();
}

/// Spurious 中断处理程序
pub extern "x86-interrupt" fn spurious_handler(frame: InterruptFrame) {
    // Spurious 中断不需要发送 EOI
    let _ = frame;
}

/// 通用中断处理程序
pub extern "x86-interrupt" fn generic_handler(frame: InterruptFrame) {
    let _ = frame;
    
    // 发送 EOI
    super::apic::local_apic_eoi();
}
