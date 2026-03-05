//! ACPI 热键输入
//!
//! 提供 ACPI 热键事件缓冲区与轮询入口。
//! 具体硬件事件采集后续可接入 ACPI EC/GPE/AML 通知路径。

use crate::drivers::acpi;
use crate::sync::{Once, OnceCell};
use core::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

/// ACPI 热键事件
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HotkeyEvent {
    PowerButton = 1,
    Sleep = 2,
    Hibernate = 3,
    VolumeUp = 4,
    VolumeDown = 5,
    VolumeMute = 6,
    BrightnessUp = 7,
    BrightnessDown = 8,
}

const HOTKEY_BUFFER_SIZE: usize = 32;

static HOTKEY_BUFFER: [AtomicU8; HOTKEY_BUFFER_SIZE] = {
    const INIT: AtomicU8 = AtomicU8::new(0);
    [INIT; HOTKEY_BUFFER_SIZE]
};

static HOTKEY_HEAD: AtomicUsize = AtomicUsize::new(0);
static HOTKEY_TAIL: AtomicUsize = AtomicUsize::new(0);
static ACPI_HOTKEY_INIT: Once = Once::new();
static PM1_EVENT_SOURCE: OnceCell<Option<Pm1EventSource>> = OnceCell::new();
static LAST_PM1_POLL_TICK: AtomicU64 = AtomicU64::new(0);

const PM1_PWRBTN_STS: u16 = 1 << 8;
const PM1_SLPBTN_STS: u16 = 1 << 9;

#[derive(Clone, Copy)]
struct Pm1EventSource {
    pm1a_evt_port: u16,
    pm1b_evt_port: u16,
    pm1_sts_len: u8,
}

/// ACPI 热键事件源信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeySourceInfo {
    pub pm1a_evt_port: u16,
    pub pm1b_evt_port: u16,
    pub pm1_sts_len: u8,
}

/// 初始化 ACPI 热键子模块
pub fn init() {
    ACPI_HOTKEY_INIT.call_once(|| {
        // 清理一次历史状态位，避免旧事件在驱动初始化后被误报。
        if let Some(source) = PM1_EVENT_SOURCE
            .get_or_init(probe_pm1_event_source)
            .as_ref()
        {
            let _ = read_and_clear_pm1_status(source.pm1a_evt_port, source.pm1_sts_len);
            if source.pm1b_evt_port != 0 {
                let _ = read_and_clear_pm1_status(source.pm1b_evt_port, source.pm1_sts_len);
            }
        }
    });
}

/// 轮询 ACPI 热键事件
pub fn poll() {
    // PM1 状态位变化速度较低，按时钟 tick 采样可减少频繁 I/O 端口访问开销。
    let now = crate::interrupt::timer_ticks();
    let last = LAST_PM1_POLL_TICK.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 1 {
        return;
    }
    LAST_PM1_POLL_TICK.store(now, Ordering::Relaxed);

    let source = PM1_EVENT_SOURCE.get_or_init(probe_pm1_event_source);
    let source = if let Some(s) = source.as_ref() {
        s
    } else {
        return;
    };

    let mut pending = read_and_clear_pm1_status(source.pm1a_evt_port, source.pm1_sts_len);
    if source.pm1b_evt_port != 0 {
        pending |= read_and_clear_pm1_status(source.pm1b_evt_port, source.pm1_sts_len);
    }

    if pending & PM1_PWRBTN_STS != 0 {
        push_event(HotkeyEvent::PowerButton);
    }
    if pending & PM1_SLPBTN_STS != 0 {
        push_event(HotkeyEvent::Sleep);
    }
}

/// 推送热键事件（供 ACPI 通知路径调用）
pub fn push_event(event: HotkeyEvent) {
    let head = HOTKEY_HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % HOTKEY_BUFFER_SIZE;

    if next_head != HOTKEY_TAIL.load(Ordering::Relaxed) {
        HOTKEY_BUFFER[head].store(event as u8, Ordering::Relaxed);
        HOTKEY_HEAD.store(next_head, Ordering::Relaxed);
    }
}

/// 读取一个热键事件
pub fn read_event() -> Option<HotkeyEvent> {
    let tail = HOTKEY_TAIL.load(Ordering::Relaxed);
    let head = HOTKEY_HEAD.load(Ordering::Relaxed);

    if tail == head {
        return None;
    }

    let raw = HOTKEY_BUFFER[tail].load(Ordering::Relaxed);
    HOTKEY_TAIL.store((tail + 1) % HOTKEY_BUFFER_SIZE, Ordering::Relaxed);
    decode_event(raw)
}

/// 检查是否有热键事件
pub fn has_event() -> bool {
    HOTKEY_TAIL.load(Ordering::Relaxed) != HOTKEY_HEAD.load(Ordering::Relaxed)
}

/// 获取缓冲区状态 (head, tail)
pub fn buffer_status() -> (usize, usize) {
    (
        HOTKEY_HEAD.load(Ordering::Relaxed),
        HOTKEY_TAIL.load(Ordering::Relaxed),
    )
}

/// 获取热键事件源信息（若 ACPI/FADT 中存在 PM1 事件块）
pub fn source_info() -> Option<HotkeySourceInfo> {
    let source = PM1_EVENT_SOURCE.get_or_init(probe_pm1_event_source);
    source.as_ref().map(|s| HotkeySourceInfo {
        pm1a_evt_port: s.pm1a_evt_port,
        pm1b_evt_port: s.pm1b_evt_port,
        pm1_sts_len: s.pm1_sts_len,
    })
}

fn decode_event(raw: u8) -> Option<HotkeyEvent> {
    match raw {
        1 => Some(HotkeyEvent::PowerButton),
        2 => Some(HotkeyEvent::Sleep),
        3 => Some(HotkeyEvent::Hibernate),
        4 => Some(HotkeyEvent::VolumeUp),
        5 => Some(HotkeyEvent::VolumeDown),
        6 => Some(HotkeyEvent::VolumeMute),
        7 => Some(HotkeyEvent::BrightnessUp),
        8 => Some(HotkeyEvent::BrightnessDown),
        _ => None,
    }
}

fn probe_pm1_event_source() -> Option<Pm1EventSource> {
    if !acpi::is_initialized() {
        return None;
    }

    let info = acpi::get_pm1_event_info()?;
    if info.pm1a_evt_blk > u16::MAX as u32 {
        return None;
    }
    if info.pm1b_evt_blk > u16::MAX as u32 {
        return None;
    }

    let pm1_sts_len = match info.pm1_sts_len {
        4 => 4,
        _ => 2,
    };

    Some(Pm1EventSource {
        pm1a_evt_port: info.pm1a_evt_blk as u16,
        pm1b_evt_port: info.pm1b_evt_blk as u16,
        pm1_sts_len,
    })
}

fn read_and_clear_pm1_status(port: u16, sts_len: u8) -> u16 {
    let status = unsafe {
        if sts_len == 4 {
            inl(port) as u16
        } else {
            inw(port)
        }
    };

    let pending = status & (PM1_PWRBTN_STS | PM1_SLPBTN_STS);
    if pending != 0 {
        unsafe {
            if sts_len == 4 {
                outl(port, pending as u32);
            } else {
                outw(port, pending);
            }
        }
    }

    pending
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    unsafe { crate::arch::inw(port) }
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    unsafe { crate::arch::inl(port) }
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    unsafe { crate::arch::outw(port, value) };
}

#[inline]
unsafe fn outl(port: u16, value: u32) {
    unsafe { crate::arch::outl(port, value) };
}
