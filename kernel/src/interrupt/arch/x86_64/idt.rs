// ============================================================================
// january_os - IDT (Interrupt Descriptor Table)
//
// x86_64 中断描述符表实现
// ============================================================================
//!
//! # 中断向量布局
//!
//! ```text
//! Vector  Name                Description
//! ────────────────────────────────────────────────────
//! 0       #DE                 Divide Error
//! 1       #DB                 Debug Exception
//! 2       NMI                 Non-Maskable Interrupt
//! 3       #BP                 Breakpoint
//! 4       #OF                 Overflow
//! 5       #BR                 BOUND Range Exceeded
//! 6       #UD                 Invalid Opcode
//! 7       #NM                 Device Not Available
//! 8       #DF                 Double Fault
//! 9       -                   (Reserved)
//! 10      #TS                 Invalid TSS
//! 11      #NP                 Segment Not Present
//! 12      #SS                 Stack Fault
//! 13      #GP                 General Protection
//! 14      #PF                 Page Fault
//! 15      -                   (Reserved)
//! 16      #MF                 x87 FPU Error
//! 17      #AC                 Alignment Check
//! 18      #MC                 Machine Check
//! 19      #XM                 SIMD Exception
//! 20      #VE                 Virtualization Exception
//! 21      #CP                 Control Protection
//! 22-31   -                   (Reserved)
//! 32-255  IRQ                 User-defined Interrupts
//! ```

use core::arch::asm;
use core::mem::size_of;
use super::gdt::KERNEL_CODE_SELECTOR;
use super::handlers;

// ============================================================================
// 中断向量号
// ============================================================================

/// 除法错误
pub const DIVIDE_ERROR: u8 = 0;
/// 调试异常
pub const DEBUG: u8 = 1;
/// NMI
pub const NMI: u8 = 2;
/// 断点
pub const BREAKPOINT: u8 = 3;
/// 溢出
pub const OVERFLOW: u8 = 4;
/// 边界检查
pub const BOUND_RANGE: u8 = 5;
/// 无效操作码
pub const INVALID_OPCODE: u8 = 6;
/// 设备不可用
pub const DEVICE_NOT_AVAILABLE: u8 = 7;
/// 双重故障
pub const DOUBLE_FAULT: u8 = 8;
/// 无效 TSS
pub const INVALID_TSS: u8 = 10;
/// 段不存在
pub const SEGMENT_NOT_PRESENT: u8 = 11;
/// 栈故障
pub const STACK_FAULT: u8 = 12;
/// 通用保护故障
pub const GENERAL_PROTECTION: u8 = 13;
/// 页错误
pub const PAGE_FAULT: u8 = 14;
/// x87 FPU 错误
pub const X87_FPU_ERROR: u8 = 16;
/// 对齐检查
pub const ALIGNMENT_CHECK: u8 = 17;
/// 机器检查
pub const MACHINE_CHECK: u8 = 18;
/// SIMD 异常
pub const SIMD_EXCEPTION: u8 = 19;
/// 虚拟化异常
pub const VIRTUALIZATION: u8 = 20;
/// 控制保护异常
pub const CONTROL_PROTECTION: u8 = 21;

/// IRQ 基础向量号
pub const IRQ_BASE: u8 = 32;

/// Timer IRQ (APIC Timer 或 PIT)
pub const IRQ_TIMER: u8 = IRQ_BASE + 0;
/// Keyboard IRQ
pub const IRQ_KEYBOARD: u8 = IRQ_BASE + 1;
/// Mouse IRQ
pub const IRQ_MOUSE: u8 = IRQ_BASE + 12;
/// COM1 串口 IRQ
pub const IRQ_COM1: u8 = IRQ_BASE + 4;
/// xHCI USB IRQ
pub const IRQ_XHCI: u8 = IRQ_BASE + 10;
/// TLB shootdown IPI
pub const IPI_TLB_SHOOTDOWN: u8 = 0xF0;
/// TLB probe IPI (for cross-CPU visibility diagnostics/tests)
pub const IPI_TLB_PROBE: u8 = 0xF1;
/// Spurious interrupt
pub const IRQ_SPURIOUS: u8 = 0xFF;

// ============================================================================
// 门描述符
// ============================================================================

/// 门类型
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum GateType {
    /// 中断门 (清除 IF)
    Interrupt = 0xE,
    /// 陷阱门 (保持 IF)
    Trap = 0xF,
}

/// IDT 门描述符 (128位)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    /// 创建空描述符
    pub const fn null() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_middle: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// 创建新的门描述符
    /// 
    /// # Arguments
    /// * `handler` - 处理程序地址
    /// * `selector` - 代码段选择子
    /// * `ist` - IST 索引 (0 = 不使用 IST, 1-7 = 使用对应 IST)
    /// * `gate_type` - 门类型
    /// * `dpl` - 描述符特权级 (0-3)
    pub fn new(handler: u64, selector: u16, ist: u8, gate_type: GateType, dpl: u8) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: ist & 0x7,
            // Present | DPL | Gate Type
            type_attr: 0x80 | ((dpl & 0x3) << 5) | (gate_type as u8),
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    /// 创建中断门
    pub fn interrupt(handler: u64) -> Self {
        Self::new(handler, KERNEL_CODE_SELECTOR, 0, GateType::Interrupt, 0)
    }

    /// 创建陷阱门
    pub fn trap(handler: u64) -> Self {
        Self::new(handler, KERNEL_CODE_SELECTOR, 0, GateType::Trap, 0)
    }

    /// 创建使用 IST 的中断门
    pub fn interrupt_ist(handler: u64, ist: u8) -> Self {
        Self::new(handler, KERNEL_CODE_SELECTOR, ist, GateType::Interrupt, 0)
    }

    /// 设置处理程序
    pub fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_middle = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        // 标记为存在
        self.type_attr |= 0x80;
    }
}

// ============================================================================
// IDT 结构
// ============================================================================

/// IDT 条目数量
const IDT_ENTRIES: usize = 256;

/// IDT 表
#[repr(C, align(16))]
pub struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

impl Idt {
    /// 创建新的 IDT
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::null(); IDT_ENTRIES],
        }
    }

    /// 设置处理程序
    pub fn set_handler(&mut self, vector: u8, entry: IdtEntry) {
        self.entries[vector as usize] = entry;
    }

    /// 获取 IDTR 值
    fn idtr(&self) -> IdtPointer {
        IdtPointer {
            limit: (size_of::<Self>() - 1) as u16,
            base: self.entries.as_ptr() as u64,
        }
    }

    /// 加载 IDT
    /// 
    /// # Safety
    /// 
    /// 调用者必须确保 IDT 已正确初始化
    pub unsafe fn load(&self) {
        let idtr = self.idtr();
        unsafe {
            asm!(
                "lidt [{}]",
                in(reg) &idtr,
                options(nostack, preserves_flags)
            );
        }
    }
}

/// IDTR 寄存器格式
#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

// ============================================================================
// 中断帧
// ============================================================================

/// 中断帧 (CPU 自动压栈的内容)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    /// 指令指针
    pub rip: u64,
    /// 代码段选择子
    pub cs: u64,
    /// 标志寄存器
    pub rflags: u64,
    /// 栈指针
    pub rsp: u64,
    /// 栈段选择子
    pub ss: u64,
}

/// 带错误码的中断帧
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrameWithError {
    /// 错误码
    pub error_code: u64,
    /// 指令指针
    pub rip: u64,
    /// 代码段选择子
    pub cs: u64,
    /// 标志寄存器
    pub rflags: u64,
    /// 栈指针
    pub rsp: u64,
    /// 栈段选择子
    pub ss: u64,
}

// ============================================================================
// 全局 IDT
// ============================================================================

/// 全局 IDT
static mut IDT: Idt = Idt::new();

/// 获取 IDT 可变引用
/// 
/// # Safety
/// 
/// 调用者必须确保没有并发访问
pub unsafe fn get_idt_mut() -> &'static mut Idt {
    unsafe { &mut *core::ptr::addr_of_mut!(IDT) }
}

/// 初始化 IDT
pub unsafe fn init() -> Result<(), &'static str> {
    let idt = get_idt_mut();

    // 设置异常处理程序
    idt.set_handler(DIVIDE_ERROR, IdtEntry::trap(
        handlers::divide_error_handler as *const () as u64
    ));
    idt.set_handler(DEBUG, IdtEntry::trap(
        handlers::debug_handler as *const () as u64
    ));
    idt.set_handler(NMI, IdtEntry::interrupt(
        handlers::nmi_handler as *const () as u64
    ));
    idt.set_handler(BREAKPOINT, IdtEntry::trap(
        handlers::breakpoint_handler as *const () as u64
    ));
    idt.set_handler(OVERFLOW, IdtEntry::trap(
        handlers::overflow_handler as *const () as u64
    ));
    idt.set_handler(BOUND_RANGE, IdtEntry::trap(
        handlers::bound_range_handler as *const () as u64
    ));
    idt.set_handler(INVALID_OPCODE, IdtEntry::trap(
        handlers::invalid_opcode_handler as *const () as u64
    ));
    idt.set_handler(DEVICE_NOT_AVAILABLE, IdtEntry::trap(
        handlers::device_not_available_handler as *const () as u64
    ));
    // Double Fault 使用 IST 1 确保有可用栈
    idt.set_handler(DOUBLE_FAULT, IdtEntry::interrupt_ist(
        handlers::double_fault_handler as *const () as u64, 1
    ));
    idt.set_handler(INVALID_TSS, IdtEntry::trap(
        handlers::invalid_tss_handler as *const () as u64
    ));
    idt.set_handler(SEGMENT_NOT_PRESENT, IdtEntry::trap(
        handlers::segment_not_present_handler as *const () as u64
    ));
    idt.set_handler(STACK_FAULT, IdtEntry::trap(
        handlers::stack_fault_handler as *const () as u64
    ));
    idt.set_handler(GENERAL_PROTECTION, IdtEntry::trap(
        handlers::general_protection_handler as *const () as u64
    ));
    idt.set_handler(PAGE_FAULT, IdtEntry::trap(
        handlers::page_fault_handler as *const () as u64
    ));
    idt.set_handler(X87_FPU_ERROR, IdtEntry::trap(
        handlers::x87_fpu_error_handler as *const () as u64
    ));
    idt.set_handler(ALIGNMENT_CHECK, IdtEntry::trap(
        handlers::alignment_check_handler as *const () as u64
    ));
    idt.set_handler(MACHINE_CHECK, IdtEntry::interrupt(
        handlers::machine_check_handler as *const () as u64
    ));
    idt.set_handler(SIMD_EXCEPTION, IdtEntry::trap(
        handlers::simd_exception_handler as *const () as u64
    ));
    idt.set_handler(VIRTUALIZATION, IdtEntry::trap(
        handlers::virtualization_handler as *const () as u64
    ));
    idt.set_handler(CONTROL_PROTECTION, IdtEntry::trap(
        handlers::control_protection_handler as *const () as u64
    ));

    // 设置硬件中断处理程序
    idt.set_handler(IRQ_TIMER, IdtEntry::interrupt(
        handlers::timer_handler as *const () as u64
    ));
    idt.set_handler(IRQ_KEYBOARD, IdtEntry::interrupt(
        handlers::keyboard_handler as *const () as u64
    ));
    idt.set_handler(IRQ_MOUSE, IdtEntry::interrupt(
        handlers::mouse_handler as *const () as u64
    ));
    idt.set_handler(IRQ_COM1, IdtEntry::interrupt(
        handlers::serial_handler as *const () as u64
    ));
    idt.set_handler(IRQ_XHCI, IdtEntry::interrupt(
        handlers::xhci_handler as *const () as u64
    ));
    idt.set_handler(IPI_TLB_SHOOTDOWN, IdtEntry::interrupt(
        handlers::tlb_shootdown_handler as *const () as u64
    ));
    idt.set_handler(IPI_TLB_PROBE, IdtEntry::interrupt(
        handlers::tlb_probe_handler as *const () as u64
    ));
    idt.set_handler(IRQ_SPURIOUS, IdtEntry::interrupt(
        handlers::spurious_handler as *const () as u64
    ));

    // 加载 IDT
    load_idt();

    Ok(())
}

/// 加载 IDT
/// 
/// # Safety
/// 
/// 只应在初始化时调用一次
pub unsafe fn load_idt() {
    unsafe {
        let idt = &*core::ptr::addr_of!(IDT);
        idt.load();
    }
}

// ============================================================================
// 中断控制
// ============================================================================

/// 启用中断
#[inline]
pub fn enable_interrupts() {
    unsafe {
        asm!("sti", options(nostack));
    }
}

/// 禁用中断
#[inline]
pub fn disable_interrupts() {
    unsafe {
        asm!("cli", options(nostack));
    }
}

/// 检查中断是否启用
#[inline]
pub fn interrupts_enabled() -> bool {
    let rflags: u64;
    unsafe {
        asm!(
            "pushfq",
            "pop {}",
            out(reg) rflags,
            options(nostack)
        );
    }
    (rflags & 0x200) != 0
}

/// 禁用中断并执行闭包
#[inline]
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let was_enabled = interrupts_enabled();
    disable_interrupts();
    let result = f();
    if was_enabled {
        enable_interrupts();
    }
    result
}

/// 等待中断
#[inline]
pub fn halt() {
    unsafe {
        asm!("hlt", options(nostack, preserves_flags));
    }
}

/// 等待中断 (启用中断后等待)
#[inline]
pub fn halt_with_interrupts() {
    unsafe {
        asm!(
            "sti",
            "hlt",
            options(nostack)
        );
    }
}
