// ============================================================================
// january_os - PIT (Programmable Interval Timer)
//
// 8254 PIT 用于 APIC Timer 校准
// ============================================================================

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// PIT 常量
// ============================================================================

/// PIT 基础频率 (Hz)
pub const PIT_FREQUENCY: u32 = 1193182;

/// PIT 通道 0 数据端口
const PIT_CHANNEL0_DATA: u16 = 0x40;
/// PIT 通道 2 数据端口
const PIT_CHANNEL2_DATA: u16 = 0x42;
/// PIT 命令端口
const PIT_COMMAND: u16 = 0x43;

/// PIT 命令: 通道 0, lo/hi, 方波模式
const PIT_CMD_CHANNEL0_SQUARE: u8 = 0x36;
/// PIT 命令: 通道 0, lo/hi, 单次模式
const PIT_CMD_CHANNEL0_ONESHOT: u8 = 0x30;
/// PIT 命令: 通道 2, lo/hi, 单次模式
const PIT_CMD_CHANNEL2_ONESHOT: u8 = 0xB0;
/// PIT 命令: 回读计数器值
const PIT_CMD_READBACK: u8 = 0xE2;

// ============================================================================
// 端口 I/O
// ============================================================================

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
    value
}

/// I/O 延迟 (等待端口稳定)
#[inline]
unsafe fn io_delay() {
    // 写入未使用的端口产生延迟
    asm!("out 0x80, al", in("al") 0u8, options(nomem, nostack));
}

// ============================================================================
// PIT 操作
// ============================================================================

/// PIT tick 计数
static PIT_TICKS: AtomicU64 = AtomicU64::new(0);

/// 设置 PIT 频率 (Hz)
pub fn pit_set_frequency(freq: u32) {
    let divisor = PIT_FREQUENCY / freq;
    let divisor = divisor.max(1).min(65535) as u16;
    
    unsafe {
        // 设置命令: 通道 0, lo/hi, 方波模式
        outb(PIT_COMMAND, PIT_CMD_CHANNEL0_SQUARE);
        io_delay();
        
        // 写入分频值 (低字节先)
        outb(PIT_CHANNEL0_DATA, (divisor & 0xFF) as u8);
        io_delay();
        outb(PIT_CHANNEL0_DATA, ((divisor >> 8) & 0xFF) as u8);
    }
}

/// 读取 PIT 计数器当前值
pub fn pit_read_count() -> u16 {
    unsafe {
        // 锁存计数器值
        outb(PIT_COMMAND, 0x00); // 锁存通道 0
        io_delay();
        
        // 读取 lo/hi
        let lo = inb(PIT_CHANNEL0_DATA) as u16;
        let hi = inb(PIT_CHANNEL0_DATA) as u16;
        
        (hi << 8) | lo
    }
}

/// PIT 中断处理 (被 IRQ0 调用)
pub fn pit_tick() {
    PIT_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// 获取 PIT tick 计数
pub fn pit_get_ticks() -> u64 {
    PIT_TICKS.load(Ordering::Relaxed)
}

/// 使用 PIT 忙等待指定微秒
/// 
/// 注意：这是一个粗略的延迟，用于校准
pub fn pit_wait_us(us: u32) {
    // 计算需要等待的 PIT 周期数
    // PIT 频率 = 1193182 Hz，即每周期约 0.838 微秒
    let cycles = (us as u64 * PIT_FREQUENCY as u64) / 1_000_000;
    
    unsafe {
        // 设置通道 2 为单次模式
        outb(PIT_COMMAND, PIT_CMD_CHANNEL2_ONESHOT);
        io_delay();
        
        // 写入计数值
        let count = cycles.min(65535) as u16;
        outb(PIT_CHANNEL2_DATA, (count & 0xFF) as u8);
        io_delay();
        outb(PIT_CHANNEL2_DATA, ((count >> 8) & 0xFF) as u8);
        
        // 等待计数器到达 0
        // 读取通道 2 状态
        loop {
            outb(PIT_COMMAND, PIT_CMD_READBACK);
            io_delay();
            let status = inb(PIT_CHANNEL2_DATA);
            if status & 0x80 != 0 {
                // OUT 引脚变高，计数完成
                break;
            }
        }
    }
}

/// 使用 PIT 忙等待指定毫秒
pub fn pit_wait_ms(ms: u32) {
    for _ in 0..ms {
        pit_wait_us(1000);
    }
}

// ============================================================================
// APIC Timer 校准
// ============================================================================

/// 校准 APIC Timer，返回每秒的 tick 数
/// 
/// 使用 PIT 作为参考时钟
pub fn calibrate_apic_timer() -> u64 {
    use super::apic;
    
    // 设置 APIC Timer 为最大计数，分频 = 16
    const APIC_TIMER_DIV: u32 = 0x03; // 除以 16
    const CALIBRATE_MS: u32 = 10;     // 校准时间 (毫秒)
    
    unsafe {
        // 设置分频
        let timer_dcr = 0xFEE00000u64 + crate::config::DIRECT_MAP_OFFSET + 0x3E0;
        core::ptr::write_volatile(timer_dcr as *mut u32, APIC_TIMER_DIV);
        
        // 设置初始计数为最大值
        let timer_icr = 0xFEE00000u64 + crate::config::DIRECT_MAP_OFFSET + 0x380;
        core::ptr::write_volatile(timer_icr as *mut u32, 0xFFFFFFFF);
        
        // 使用 PIT 等待指定时间
        pit_wait_ms(CALIBRATE_MS);
        
        // 读取当前计数
        let timer_ccr = 0xFEE00000u64 + crate::config::DIRECT_MAP_OFFSET + 0x390;
        let elapsed = 0xFFFFFFFF - core::ptr::read_volatile(timer_ccr as *const u32);
        
        // 停止 Timer
        core::ptr::write_volatile(timer_icr as *mut u32, 0);
        
        // 计算每秒 tick 数
        // elapsed 是 CALIBRATE_MS 毫秒内的 tick 数
        // 频率 = elapsed * 1000 / CALIBRATE_MS * 16 (分频因子)
        let ticks_per_sec = (elapsed as u64 * 1000 / CALIBRATE_MS as u64) * 16;
        
        ticks_per_sec
    }
}
