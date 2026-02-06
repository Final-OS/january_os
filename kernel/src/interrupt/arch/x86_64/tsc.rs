// ============================================================================
// january_os - TSC (Time Stamp Counter)
// ============================================================================

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::drivers::acpi;

/// TSC 频率 (Hz)
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);

/// ACPI PM Timer 频率 (3.579545 MHz)
const PM_TIMER_FREQUENCY: u64 = 3_579_545;

/// 读取 TSC
#[inline]
pub fn rdtsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// 读取 TSC (串行化)
#[inline]
pub fn rdtscp() -> (u64, u32) {
    let mut aux: u32 = 0;
    let tsc = unsafe { core::arch::x86_64::__rdtscp(&mut aux) };
    (tsc, aux)
}

/// 获取 TSC 频率
pub fn tsc_frequency() -> u64 {
    TSC_FREQUENCY.load(Ordering::Relaxed)
}

/// 端口输入 (32位)
#[inline]
unsafe fn inl(port: u16) -> u32 {
    let ret: u32;
    asm!("in eax, dx", out("eax") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// 使用 ACPI PM Timer 校准 TSC
pub fn calibrate_tsc() {
    // 尝试获取 FADT 表
    let fadt = match acpi::find_table::<acpi::Fadt>() {
        Some(t) => t,
        None => {
            crate::warn!("TSC: FADT not found, cannot calibrate TSC using PM Timer");
            return;
        }
    };

    let pm_timer_port = fadt.pm_timer_blk as u16;
    if pm_timer_port == 0 {
        crate::warn!("TSC: PM Timer block not present in FADT");
        return;
    }

    // 检查 PM Timer 是否为 32 位 (FADT flags bit 8)
    // Bit 8: TMR_VAL_EXT (1 = 32-bit, 0 = 24-bit)
    let is_32bit = (fadt.flags & (1 << 8)) != 0;
    let mask = if is_32bit { 0xFFFFFFFF } else { 0x00FFFFFF };

    crate::info!("TSC: Calibrating using ACPI PM Timer at port {:#x} ({} bit)...", pm_timer_port, if is_32bit { 32 } else { 24 });

    // 校准时长: 100ms (约 357954 ticks)
    let calibration_ticks = PM_TIMER_FREQUENCY / 10; 
    
    unsafe {
        let start_pm = inl(pm_timer_port) & mask;
        let start_tsc = rdtsc();
        
        loop {
            let current_pm = inl(pm_timer_port) & mask;
            
            // 处理溢出
            let elapsed_pm = if current_pm >= start_pm {
                current_pm - start_pm
            } else {
                (current_pm + (mask + 1)) - start_pm
            };
            
            if elapsed_pm >= calibration_ticks as u32 {
                break;
            }
        }
        
        let end_tsc = rdtsc();
        let diff_tsc = end_tsc - start_tsc;
        
        // 计算频率: diff_tsc * 10
        let freq = diff_tsc * 10;
        TSC_FREQUENCY.store(freq, Ordering::SeqCst);
        
        crate::ok!("TSC: Frequency detected: {} Hz ({} MHz)", freq, freq / 1_000_000);
    }
}
