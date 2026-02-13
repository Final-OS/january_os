//! x86_64 中断和异常处理
//!
//! 包含 GDT, IDT, APIC, PIT 等硬件支持

pub mod apic;
pub mod gdt;
pub mod idt;
pub mod handlers;
pub mod tsc;

use core::sync::atomic::{AtomicBool, Ordering};
use crate::mm;
use gdt::{init_gdt, set_interrupt_stack};
use apic::{init_local_apic, init_ioapic, ioapic_set_irq, ioapic_unmask_irq};
use idt::IRQ_KEYBOARD; // Only need constants used in init
use idt::{IRQ_MOUSE, IRQ_COM1};

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
    /// 直接映射区基地址 (用于计算 IST 栈虚拟地址)
    pub direct_map_base: u64,
}

/// 初始化中断子系统 (BSP)
///
/// # Arguments
/// * `info` - 初始化信息
///
/// # Safety
///
/// 必须在单核环境下调用（通常是 BSP 初始化阶段），中断禁用。
/// 此函数执行全局硬件初始化（如 I/O APIC），AP 核心启动时不应调用此函数。
pub unsafe fn init_bsp(info: &InterruptInitInfo) -> Result<(), &'static str> {
    if INITIALIZED.load(Ordering::Relaxed) {
        return Err("Interrupt subsystem already initialized");
    }

    // 1. 初始化 GDT 和 TSS (BSP ID = 0)
    init_gdt(0, info.kernel_stack_top);

    // 1.0 初始化 syscall 指令入口 (MSR: STAR/LSTAR/SFMASK/EFER.SCE)
    crate::arch::syscall::init_syscall();

    // 1.1 分配并设置 IST1 栈 (用于 Double Fault)
    // 分配 4 个页 (16KB)
    if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
        let ist_top = info.direct_map_base + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
        set_interrupt_stack(0, 1, ist_top);
    }

    // 2. 初始化 IDT (设置表项并加载)
    idt::init()?;

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

/// 兼容旧代码的别名，指向 init_bsp
pub unsafe fn init(info: &InterruptInitInfo) -> Result<(), &'static str> {
    init_bsp(info)
}

/// 初始化中断子系统 (AP)
///
/// # Arguments
/// * `cpu_id` - CPU ID (必须唯一且 < MAX_CPUS)
/// * `kernel_stack_top` - 该 CPU 的内核栈顶地址
/// * `local_apic_addr` - Local APIC 物理地址
/// * `direct_map_base` - 直接映射区基地址 (用于计算 IST 栈虚拟地址)
///
/// # Safety
///
/// 必须在 AP 核心启动时调用，中断禁用。
pub unsafe fn init_ap(cpu_id: usize, kernel_stack_top: u64, local_apic_addr: u64, direct_map_base: u64) -> Result<(), &'static str> {
    // 1. 初始化 GDT 和 TSS (Local)
    init_gdt(cpu_id, kernel_stack_top);

    // 1.0 AP 同步初始化 syscall 指令入口
    crate::arch::syscall::init_syscall();

    // 1.1 分配并设置 IST1 栈 (用于 Double Fault)
    if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
        let ist_top = direct_map_base + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
        set_interrupt_stack(cpu_id, 1, ist_top);
    }

    // 2. 加载 IDT (Global IDT, Local IDTR)
    idt::load_idt();

    // 3. 初始化 Local APIC (Local)
    if local_apic_addr != 0 {
        init_local_apic(local_apic_addr);
    }

    Ok(())
}

/// 检查中断子系统是否已初始化
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}
