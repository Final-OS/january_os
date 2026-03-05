//! x86_64 中断和异常处理
//!
//! 包含 GDT, IDT, APIC, PIT 等硬件支持

pub mod apic;
pub mod gdt;
pub mod handlers;
pub mod idt;
pub mod tsc;

use crate::mm;
use apic::{init_ioapic, init_local_apic, ioapic_set_irq, ioapic_unmask_irq};
use core::sync::atomic::{AtomicBool, Ordering};
use gdt::{init_gdt, set_interrupt_stack};
use idt::IRQ_KEYBOARD; // Only need constants used in init
use idt::{IRQ_COM1, IRQ_MOUSE};

/// 中断子系统是否已初始化
static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
pub struct IrqRouteOverride {
    pub source: u8,
    pub gsi: u32,
    pub level_triggered: bool,
    pub active_low: bool,
}

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
    /// MADT Interrupt Source Override 条目数量
    pub irq_override_count: usize,
    /// MADT Interrupt Source Override 列表（仅 ISA IRQ）
    pub irq_overrides: [IrqRouteOverride; 16],
    /// 直接映射区基地址 (用于计算 IST 栈虚拟地址)
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

    // ISA 默认：IRQx -> GSI x, edge/high.
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

        // ISA 中断路由：优先按 MADT Interrupt Source Override，未命中则回退默认 edge/high。
        setup_isa_irq(info, 1, IRQ_KEYBOARD, "keyboard");
        setup_isa_irq(info, 12, IRQ_MOUSE, "mouse");
        setup_isa_irq(info, 4, IRQ_COM1, "serial");
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
pub unsafe fn init_ap(
    cpu_id: usize,
    kernel_stack_top: u64,
    local_apic_addr: u64,
    direct_map_base: u64,
) -> Result<(), &'static str> {
    crate::smp::ap_boot_probe_set_stage(31);

    // 1. 初始化 GDT 和 TSS (Local)
    init_gdt(cpu_id, kernel_stack_top);
    crate::smp::ap_boot_probe_set_stage(32);

    // 1.0 AP 同步初始化 syscall 指令入口
    crate::arch::syscall::init_syscall();
    crate::smp::ap_boot_probe_set_stage(33);

    // 1.1 先初始化本核 Local APIC，确保后续可能涉及锁/CPU 标识的路径可用。
    if local_apic_addr != 0 {
        init_local_apic(local_apic_addr);
    }
    crate::smp::ap_boot_probe_set_stage(34);

    // 1.2 分配并设置 IST1 栈 (用于 Double Fault)
    if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
        let ist_top = direct_map_base + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
        set_interrupt_stack(cpu_id, 1, ist_top);
    }
    crate::smp::ap_boot_probe_set_stage(35);

    // 2. 加载 IDT (Global IDT, Local IDTR)
    idt::load_idt();
    crate::smp::ap_boot_probe_set_stage(36);

    crate::smp::ap_boot_probe_set_stage(37);
    Ok(())
}

/// 检查中断子系统是否已初始化
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}
