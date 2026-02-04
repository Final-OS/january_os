// ============================================================================
// january_os - APIC (Advanced Programmable Interrupt Controller)
//
// Local APIC 和 I/O APIC 支持
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
const LAPIC_ICR_LO: u32 = 0x300;
/// Interrupt Command Register (high)
const LAPIC_ICR_HI: u32 = 0x310;
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
// 全局状态
// ============================================================================

/// Local APIC 状态
struct LocalApicState {
    /// 基地址 (虚拟地址)
    base: u64,
}

/// Local APIC 状态（一次性初始化）
static LOCAL_APIC: OnceCell<LocalApicState> = OnceCell::new();

/// Timer 频率 (ticks per second)
static TIMER_FREQUENCY: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Local APIC 操作
// ============================================================================

/// 获取 Local APIC 基地址
#[inline]
fn lapic_base() -> Option<u64> {
    LOCAL_APIC.get().map(|s| s.base)
}

/// 读取 Local APIC 寄存器
#[inline]
fn lapic_read(reg: u32) -> u32 {
    let base = match lapic_base() {
        Some(b) => b,
        None => return 0,
    };
    unsafe {
        core::ptr::read_volatile((base + reg as u64) as *const u32)
    }
}

/// 写入 Local APIC 寄存器
#[inline]
fn lapic_write(reg: u32, value: u32) {
    let base = match lapic_base() {
        Some(b) => b,
        None => return,
    };
    unsafe {
        core::ptr::write_volatile((base + reg as u64) as *mut u32, value);
    }
}

/// 初始化 Local APIC
///
/// # Arguments
/// * `apic_base_phys` - Local APIC 物理基地址 (通常从 MADT 获取)
pub fn init_local_apic(apic_base_phys: u64) {
    // 转换为虚拟地址
    let apic_base_virt = apic_base_phys + crate::config::DIRECT_MAP_OFFSET;
    
    // 设置状态（只会执行一次）
    let _ = LOCAL_APIC.set(LocalApicState {
        base: apic_base_virt,
    });

    // 启用 APIC 并设置 spurious 向量
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
    lapic_read(LAPIC_ID) >> 24
}

/// 检查 APIC 是否已初始化
pub fn apic_initialized() -> bool {
    LOCAL_APIC.is_initialized()
}

// ============================================================================
// APIC Timer
// ============================================================================

/// APIC Timer 每秒 tick 数 (校准后填充)
static APIC_TIMER_TICKS_PER_SEC: AtomicU64 = AtomicU64::new(0);

/// 校准并初始化 APIC Timer
/// 
/// 返回校准得到的总线频率 (Hz)
pub fn calibrate_timer() -> u64 {
    if !apic_initialized() {
        return 0;
    }
    
    // 使用 PIT 校准
    let ticks_per_sec = super::pit::calibrate_apic_timer();
    APIC_TIMER_TICKS_PER_SEC.store(ticks_per_sec, Ordering::SeqCst);
    
    ticks_per_sec
}

/// 初始化 APIC Timer
///
/// # Arguments
/// * `vector` - 中断向量号
/// * `frequency_hz` - 期望的中断频率 (Hz)
pub fn init_apic_timer(vector: u8, frequency_hz: u32) {
    if !apic_initialized() {
        return;
    }

    let ticks_per_sec = APIC_TIMER_TICKS_PER_SEC.load(Ordering::Relaxed);
    if ticks_per_sec == 0 {
        // 未校准，使用默认值
        // 假设 ~100MHz 总线频率
        let initial_count = 100_000_000 / frequency_hz;
        lapic_write(LAPIC_TIMER_DCR, TIMER_DIV_16);
        lapic_write(LAPIC_TIMER, TIMER_PERIODIC | vector as u32);
        lapic_write(LAPIC_TIMER_ICR, initial_count);
    } else {
        // 使用校准值
        let initial_count = (ticks_per_sec / frequency_hz as u64) as u32;
        lapic_write(LAPIC_TIMER_DCR, TIMER_DIV_1);
        lapic_write(LAPIC_TIMER, TIMER_PERIODIC | vector as u32);
        lapic_write(LAPIC_TIMER_ICR, initial_count);
    }

    TIMER_FREQUENCY.store(frequency_hz as u64, Ordering::SeqCst);
}

/// 获取 APIC Timer 频率 (ticks per second)
pub fn apic_timer_frequency() -> u64 {
    APIC_TIMER_TICKS_PER_SEC.load(Ordering::Relaxed)
}

/// 停止 APIC Timer
pub fn stop_apic_timer() {
    lapic_write(LAPIC_TIMER, 0x10000); // Masked
    lapic_write(LAPIC_TIMER_ICR, 0);
}

/// 获取 Timer 当前计数
pub fn apic_timer_current() -> u32 {
    lapic_read(LAPIC_TIMER_CCR)
}

// ============================================================================
// IPI (Inter-Processor Interrupt)
// ============================================================================

/// IPI 发送模式
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum IpiDeliveryMode {
    Fixed = 0b000,
    LowestPriority = 0b001,
    Smi = 0b010,
    Nmi = 0b100,
    Init = 0b101,
    StartUp = 0b110,
}

/// 发送 IPI 到指定 CPU
pub fn send_ipi(apic_id: u32, vector: u8, mode: IpiDeliveryMode) {
    // 设置目标 APIC ID
    lapic_write(LAPIC_ICR_HI, apic_id << 24);
    
    // 发送 IPI
    let icr_lo = vector as u32 | ((mode as u32) << 8);
    lapic_write(LAPIC_ICR_LO, icr_lo);
    
    // 等待发送完成
    while lapic_read(LAPIC_ICR_LO) & (1 << 12) != 0 {
        core::hint::spin_loop();
    }
}

/// 发送 IPI 到所有 CPU (除自己)
pub fn send_ipi_all_excluding_self(vector: u8) {
    let icr_lo = vector as u32 | (0b11 << 18); // All excluding self
    lapic_write(LAPIC_ICR_LO, icr_lo);
}

// ============================================================================
// I/O APIC
// ============================================================================

/// I/O APIC 寄存器选择
const IOAPIC_REG_SEL: u32 = 0x00;
/// I/O APIC 数据寄存器
const IOAPIC_REG_WIN: u32 = 0x10;

/// I/O APIC ID 寄存器
const IOAPIC_ID: u32 = 0x00;
/// I/O APIC 版本寄存器
const IOAPIC_VER: u32 = 0x01;
/// I/O APIC 仲裁 ID
const IOAPIC_ARB: u32 = 0x02;
/// I/O APIC 重定向表基址
const IOAPIC_REDTBL: u32 = 0x10;

/// I/O APIC 状态
struct IoApicState {
    /// 基地址 (虚拟地址)
    base: u64,
}

/// I/O APIC 状态（一次性初始化）
static IO_APIC: OnceCell<IoApicState> = OnceCell::new();

/// 获取 I/O APIC 基地址
#[inline]
fn ioapic_base() -> Option<u64> {
    IO_APIC.get().map(|s| s.base)
}

/// 读取 I/O APIC 寄存器
fn ioapic_read(reg: u32) -> u32 {
    let base = match ioapic_base() {
        Some(b) => b,
        None => return 0,
    };
    unsafe {
        core::ptr::write_volatile((base + IOAPIC_REG_SEL as u64) as *mut u32, reg);
        core::ptr::read_volatile((base + IOAPIC_REG_WIN as u64) as *const u32)
    }
}

/// 写入 I/O APIC 寄存器
fn ioapic_write(reg: u32, value: u32) {
    let base = match ioapic_base() {
        Some(b) => b,
        None => return,
    };
    unsafe {
        core::ptr::write_volatile((base + IOAPIC_REG_SEL as u64) as *mut u32, reg);
        core::ptr::write_volatile((base + IOAPIC_REG_WIN as u64) as *mut u32, value);
    }
}

/// 初始化 I/O APIC
pub fn init_ioapic(ioapic_base_phys: u64, gsi_base: u32) {
    let ioapic_base_virt = ioapic_base_phys + crate::config::DIRECT_MAP_OFFSET;
    
    // 设置状态（只会执行一次）
    let _ = IO_APIC.set(IoApicState {
        base: ioapic_base_virt,
    });

    // 读取 I/O APIC 版本以获取最大重定向条目数
    let ver = ioapic_read(IOAPIC_VER);
    let max_entries = ((ver >> 16) & 0xFF) + 1;

    // 屏蔽所有中断
    for i in 0..max_entries {
        let reg = IOAPIC_REDTBL + i * 2;
        // 设置为屏蔽 (bit 16 = masked)
        ioapic_write(reg, 0x10000);
        ioapic_write(reg + 1, 0);
    }

    let _ = gsi_base;
}

/// 设置 I/O APIC 重定向条目
///
/// # Arguments
/// * `irq` - IRQ 号 (相对于 GSI base)
/// * `vector` - 目标中断向量
/// * `dest_apic_id` - 目标 Local APIC ID
/// * `level_triggered` - 是否电平触发
/// * `active_low` - 是否低电平有效
pub fn ioapic_set_irq(
    irq: u8,
    vector: u8,
    dest_apic_id: u8,
    level_triggered: bool,
    active_low: bool,
) {
    let reg = IOAPIC_REDTBL + (irq as u32) * 2;
    
    let mut entry: u64 = vector as u64;
    
    // Delivery mode: Fixed (000)
    // Destination mode: Physical (0)
    
    if level_triggered {
        entry |= 1 << 15; // Level triggered
    }
    if active_low {
        entry |= 1 << 13; // Active low
    }
    
    // Destination APIC ID
    entry |= (dest_apic_id as u64) << 56;
    
    ioapic_write(reg, entry as u32);
    ioapic_write(reg + 1, (entry >> 32) as u32);
}

/// 屏蔽 I/O APIC 中断
pub fn ioapic_mask_irq(irq: u8) {
    let reg = IOAPIC_REDTBL + (irq as u32) * 2;
    let entry = ioapic_read(reg);
    ioapic_write(reg, entry | 0x10000);
}

/// 取消屏蔽 I/O APIC 中断
pub fn ioapic_unmask_irq(irq: u8) {
    let reg = IOAPIC_REDTBL + (irq as u32) * 2;
    let entry = ioapic_read(reg);
    ioapic_write(reg, entry & !0x10000);
}
