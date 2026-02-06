// ============================================================================
// january_os - APIC (Advanced Programmable Interrupt Controller)
//
// Local APIC 和 I/O APIC 支持 (x2APIC supported)
// ============================================================================

use core::sync::atomic::{AtomicU64, Ordering};
use crate::sync::OnceCell;

// ============================================================================
// Local APIC 寄存器偏移
// ============================================================================

/// Local APIC ID
const LAPIC_ID: u32 = 0x020;
/// Local APIC 版本
const LAPIC_VER: u32 = 0x030;
/// Task Priority Register
const LAPIC_TPR: u32 = 0x080;
/// EOI Register
const LAPIC_EOI: u32 = 0x0B0;
/// Spurious Interrupt Vector Register
const LAPIC_SVR: u32 = 0x0F0;
/// Error Status Register
const LAPIC_ESR: u32 = 0x280;
/// Interrupt Command Register (low)
pub const LAPIC_ICR_LO: u32 = 0x300;
/// Interrupt Command Register (high)
pub const LAPIC_ICR_HI: u32 = 0x310;
/// LVT Timer
const LAPIC_TIMER: u32 = 0x320;
/// LVT Thermal Sensor
const LAPIC_THERMAL: u32 = 0x330;
/// LVT Performance Counter
const LAPIC_PERF: u32 = 0x340;
/// LVT LINT0
const LAPIC_LINT0: u32 = 0x350;
/// LVT LINT1
const LAPIC_LINT1: u32 = 0x360;
/// LVT Error
const LAPIC_ERROR: u32 = 0x370;
/// Timer Initial Count
const LAPIC_TIMER_ICR: u32 = 0x380;
/// Timer Current Count
const LAPIC_TIMER_CCR: u32 = 0x390;
/// Timer Divide Configuration
const LAPIC_TIMER_DCR: u32 = 0x3E0;

// MSR Constants
const IA32_APIC_BASE: u32 = 0x1B;
const IA32_APIC_BASE_BSP: u64 = 1 << 8;
const IA32_APIC_BASE_EXTD: u64 = 1 << 10;
const IA32_APIC_BASE_ENABLE: u64 = 1 << 11;

// ============================================================================
// IPI (Inter-Processor Interrupt) Constants
// ============================================================================

// Delivery Mode
pub const ICR_DELIVERY_FIXED: u32 = 0x000;
pub const ICR_DELIVERY_LOWEST: u32 = 0x100;
pub const ICR_DELIVERY_SMI: u32 = 0x200;
pub const ICR_DELIVERY_NMI: u32 = 0x400;
pub const ICR_DELIVERY_INIT: u32 = 0x500;
pub const ICR_DELIVERY_STARTUP: u32 = 0x600;

// Destination Mode
pub const ICR_DEST_PHYSICAL: u32 = 0x000;
pub const ICR_DEST_LOGICAL: u32 = 0x800;

// Delivery Status
pub const ICR_DELIVERY_STATUS_IDLE: u32 = 0x0000;
pub const ICR_DELIVERY_STATUS_PENDING: u32 = 0x1000;

// Level
pub const ICR_LEVEL_DEASSERT: u32 = 0x0000;
pub const ICR_LEVEL_ASSERT: u32 = 0x4000;

// Trigger Mode
pub const ICR_TRIGGER_EDGE: u32 = 0x0000;
pub const ICR_TRIGGER_LEVEL: u32 = 0x8000;

// Destination Shorthand
pub const ICR_SHORTHAND_NONE: u32 = 0x00000;
pub const ICR_SHORTHAND_SELF: u32 = 0x40000;
pub const ICR_SHORTHAND_ALL: u32 = 0x80000;
pub const ICR_SHORTHAND_ALL_BUT_SELF: u32 = 0xC0000;

// ============================================================================
// APIC 常量
// ============================================================================

/// APIC 软件启用位
const APIC_SW_ENABLE: u32 = 0x100;
/// Spurious 向量号
const SPURIOUS_VECTOR: u32 = 0xFF;

/// Timer 模式 - 周期性
const TIMER_PERIODIC: u32 = 0x20000;
/// Timer 模式 - 一次性
const TIMER_ONE_SHOT: u32 = 0x00000;

/// Timer 分频因子
const TIMER_DIV_1: u32 = 0xB;
const TIMER_DIV_16: u32 = 0x3;
const TIMER_DIV_128: u32 = 0xA;

// ============================================================================
// Helper Functions
// ============================================================================

unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high);
    ((high as u64) << 32) | (low as u64)
}

unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high);
}

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val);
}

pub unsafe fn disable_8259_pic() {
    // Mask all interrupts on both PICs
    outb(0x21, 0xff);
    outb(0xa1, 0xff);
}

fn check_x2apic_support() -> bool {
    use core::arch::x86_64::__cpuid;
    let res = unsafe { __cpuid(1) };
    (res.ecx & (1 << 21)) != 0
}

// ============================================================================
// 全局状态
// ============================================================================

/// Local APIC 状态
#[derive(Debug, Clone, Copy)]
struct LocalApicState {
    /// 基地址 (虚拟地址) - 仅在 xAPIC 模式下使用
    base: u64,
    /// 是否使用 x2APIC 模式
    x2apic: bool,
}

/// Local APIC 状态（一次性初始化）
static LOCAL_APIC: OnceCell<LocalApicState> = OnceCell::new();

/// Timer 频率 (ticks per second)
static TIMER_FREQUENCY: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Local APIC 操作
// ============================================================================

/// 获取 Local APIC 状态
#[inline]
fn get_lapic_state() -> Option<LocalApicState> {
    LOCAL_APIC.get().cloned()
}

/// 读取 Local APIC 寄存器
#[inline]
pub fn lapic_read(reg: u32) -> u32 {
    let state = match get_lapic_state() {
        Some(s) => s,
        None => return 0,
    };
    
    if state.x2apic {
        // MSR address = 0x800 + (reg >> 4)
        unsafe { rdmsr(0x800 + (reg >> 4)) as u32 }
    } else {
        unsafe {
            core::ptr::read_volatile((state.base + reg as u64) as *const u32)
        }
    }
}

/// 写入 Local APIC 寄存器
#[inline]
pub fn lapic_write(reg: u32, value: u32) {
    let state = match get_lapic_state() {
        Some(s) => s,
        None => return,
    };
    
    if state.x2apic {
        // ICR (0x300) needs special handling in x2APIC if using high/low pair,
        // but typically we should use send_ipi for ICR.
        // For standard registers, write to MSR.
        if reg == LAPIC_ICR_LO || reg == LAPIC_ICR_HI {
             // Use send_ipi instead
             return;
        }
        unsafe { wrmsr(0x800 + (reg >> 4), value as u64) };
    } else {
        unsafe {
            core::ptr::write_volatile((state.base + reg as u64) as *mut u32, value);
        }
    }
}

/// 初始化 Local APIC
///
/// # Arguments
/// * `apic_base_phys` - Local APIC 物理基地址 (通常从 MADT 获取)
pub fn init_local_apic(apic_base_phys: u64) {
    // 禁用 8259 PIC
    unsafe { disable_8259_pic(); }

    // 1. Per-CPU: Enable x2APIC/xAPIC
    let x2apic_supported = check_x2apic_support();
    let mut x2apic_enabled = false;
    
    if x2apic_supported {
        unsafe {
            let base_msr = rdmsr(IA32_APIC_BASE);
            wrmsr(IA32_APIC_BASE, base_msr | IA32_APIC_BASE_EXTD | IA32_APIC_BASE_ENABLE);
        }
        x2apic_enabled = true;
    } else {
        // Enable xAPIC (ensure bit 11 is set)
        unsafe {
            let base_msr = rdmsr(IA32_APIC_BASE);
            wrmsr(IA32_APIC_BASE, base_msr | IA32_APIC_BASE_ENABLE);
        }
    }

    // 2. Global State: Set OnceCell (only first time, BSP)
    if !LOCAL_APIC.is_initialized() {
        let apic_base_virt = apic_base_phys + crate::config::DIRECT_MAP_OFFSET;
        let _ = LOCAL_APIC.set(LocalApicState {
            base: apic_base_virt,
            x2apic: x2apic_enabled,
        });
        if x2apic_enabled {
            crate::kprintln!("      [APIC] x2APIC enabled");
        } else {
            crate::kprintln!("      [APIC] xAPIC enabled (Legacy)");
        }
    }

    // 3. Per-CPU: Configure LVT, TPR, SVR
    lapic_write(LAPIC_SVR, APIC_SW_ENABLE | SPURIOUS_VECTOR);

    // 清除错误状态
    lapic_write(LAPIC_ESR, 0);
    lapic_write(LAPIC_ESR, 0);

    // 设置 Task Priority 为 0 (接受所有中断)
    lapic_write(LAPIC_TPR, 0);

    // 禁用 LVT 条目 (后续按需启用)
    lapic_write(LAPIC_TIMER, 0x10000);    // Masked
    lapic_write(LAPIC_THERMAL, 0x10000);  // Masked
    lapic_write(LAPIC_PERF, 0x10000);     // Masked
    lapic_write(LAPIC_LINT0, 0x10000);    // Masked
    lapic_write(LAPIC_LINT1, 0x10000);    // Masked
    lapic_write(LAPIC_ERROR, 0x10000);    // Masked

    // 发送 EOI 清除任何挂起的中断
    lapic_write(LAPIC_EOI, 0);
}

/// 发送 EOI (End of Interrupt)
#[inline]
pub fn local_apic_eoi() {
    lapic_write(LAPIC_EOI, 0);
}

/// 获取当前 CPU 的 APIC ID
pub fn local_apic_id() -> u32 {
    let val = lapic_read(LAPIC_ID);
    if let Some(state) = get_lapic_state() {
        if state.x2apic {
            return val;
        }
    }
    val >> 24
}

/// 检查 APIC 是否已初始化
pub fn apic_initialized() -> bool {
    LOCAL_APIC.is_initialized()
}

// ============================================================================
// IPI Functions
// ============================================================================

/// 等待 IPI 发送完成 (等待 ICR 空闲)
#[inline]
pub fn wait_for_ipi_delivery() {
    let state = match get_lapic_state() {
        Some(s) => s,
        None => return,
    };
    
    if state.x2apic {
        // x2APIC has no delivery status bit, it waits automatically? 
        // Intel SDM says: "The delivery status bit is not supported in x2APIC mode."
        // "Software does not need to check the delivery status bit..."
        return;
    }

    // 等待 Delivery Status bit (bit 12) 变清楚
    while lapic_read(LAPIC_ICR_LO) & ICR_DELIVERY_STATUS_PENDING != 0 {
        core::hint::spin_loop();
    }
}

/// 发送 IPI (底层函数)
pub fn send_ipi(
    apic_id: u32,
    vector: u8,
    delivery_mode: u32,
    shorthand: u32,
    level: u32,
    trigger: u32,
) {
    let state = match get_lapic_state() {
        Some(s) => s,
        None => return,
    };

    wait_for_ipi_delivery();

    let icr_low = (vector as u32) |
                  delivery_mode |
                  ICR_DEST_PHYSICAL |
                  level |
                  trigger |
                  shorthand;

    if state.x2apic {
        // x2APIC: 64-bit MSR write to 0x830
        let dest = if shorthand == ICR_SHORTHAND_NONE {
             (apic_id as u64) << 32 
        } else {
             0
        };
        let icr = dest | (icr_low as u64);
        unsafe { wrmsr(0x830, icr) };
    } else {
        // Legacy xAPIC
        if shorthand == ICR_SHORTHAND_NONE {
            lapic_write(LAPIC_ICR_HI, apic_id << 24);
        }
        lapic_write(LAPIC_ICR_LO, icr_low);
    }
}

/// 发送 INIT IPI
pub fn send_init_ipi(apic_id: u32) {
    send_ipi(
        apic_id,
        0, // INIT IPI 忽略 vector
        ICR_DELIVERY_INIT,
        ICR_SHORTHAND_NONE,
        ICR_LEVEL_ASSERT,
        ICR_TRIGGER_EDGE,
    );
}

/// 发送 SIPI (Start-up IPI)
pub fn send_sipi(apic_id: u32, vector: u8) {
    send_ipi(
        apic_id,
        vector,
        ICR_DELIVERY_STARTUP,
        ICR_SHORTHAND_NONE,
        ICR_LEVEL_ASSERT,
        ICR_TRIGGER_EDGE,
    );
}

// ============================================================================
// APIC Timer
// ============================================================================

/// APIC Timer 每秒 tick 数 (校准后填充)
static APIC_TIMER_TICKS_PER_SEC: AtomicU64 = AtomicU64::new(0);

/// 校准并初始化 APIC Timer
pub fn calibrate_timer() -> u64 {
    if !apic_initialized() {
        return 0;
    }
    
    // 使用 PIT 校准
    let ticks_per_sec = super::pit::calibrate_apic_timer();
    APIC_TIMER_TICKS_PER_SEC.store(ticks_per_sec, Ordering::SeqCst);
    
    ticks_per_sec
}

pub fn apic_timer_frequency() -> u64 {
    APIC_TIMER_TICKS_PER_SEC.load(Ordering::SeqCst)
}

pub fn init_apic_timer(vector: u8, hz: u32) {
    let ticks_per_sec = apic_timer_frequency();
    let initial_count = if ticks_per_sec > 0 {
        ticks_per_sec / hz as u64
    } else {
        // Fallback if not calibrated
        1000000 
    };

    let mode = TIMER_PERIODIC;
    lapic_write(LAPIC_TIMER, (vector as u32) | mode);
    lapic_write(LAPIC_TIMER_DCR, TIMER_DIV_16);
    lapic_write(LAPIC_TIMER_ICR, initial_count as u32);
}

pub fn stop_apic_timer() {
    lapic_write(LAPIC_TIMER_ICR, 0);
}

// ============================================================================
// I/O APIC (Minimal support)
// ============================================================================

// I/O APIC registers
const IOAPICID: u32 = 0x00;
const IOAPICVER: u32 = 0x01;
const IOAPICARB: u32 = 0x02;
const IOREDTBL: u32 = 0x10;

struct IoApicState {
    base: u64,
    gsi_base: u32,
}

static IO_APIC: OnceCell<IoApicState> = OnceCell::new();

pub fn init_ioapic(addr: u64, gsi_base: u32) {
     let addr_virt = addr + crate::config::DIRECT_MAP_OFFSET;
     let _ = IO_APIC.set(IoApicState {
         base: addr_virt,
         gsi_base,
     });
     // Mask all? No, we set them up as needed.
}

unsafe fn ioapic_read(reg: u32) -> u32 {
    let state = match IO_APIC.get() {
        Some(s) => s,
        None => return 0,
    };
    let base = state.base as *mut u32;
    core::ptr::write_volatile(base, reg);
    core::ptr::read_volatile(base.add(4))
}

unsafe fn ioapic_write(reg: u32, value: u32) {
    let state = match IO_APIC.get() {
        Some(s) => s,
        None => return,
    };
    let base = state.base as *mut u32;
    core::ptr::write_volatile(base, reg);
    core::ptr::write_volatile(base.add(4), value);
}

pub fn ioapic_set_irq(irq: u8, vector: u8, dest: u8, level_triggered: bool, active_low: bool) {
    // Write to IOAPIC redirection table
    let reg = IOREDTBL + 2 * (irq as u32);
    
    let mut low = vector as u32;
    if !level_triggered {
        low |= 0; // Edge
    } else {
        low |= 1 << 15; // Level
    }
    
    if !active_low {
        low |= 0; // High active
    } else {
        low |= 1 << 13; // Low active
    }
    
    low |= 1 << 16; // Masked initially
    
    let high = (dest as u32) << 24;
    
    unsafe {
        ioapic_write(reg, low);
        ioapic_write(reg + 1, high);
    }
}

pub fn ioapic_unmask_irq(irq: u8) {
    let reg = IOREDTBL + 2 * (irq as u32);
    unsafe {
        let low = ioapic_read(reg);
        ioapic_write(reg, low & !(1 << 16));
    }
}

pub fn ioapic_mask_irq(irq: u8) {
    let reg = IOREDTBL + 2 * (irq as u32);
    unsafe {
        let low = ioapic_read(reg);
        ioapic_write(reg, low | (1 << 16));
    }
}
