//! 内核运行时虚拟地址布局
//!
//! 作为 direct-map/vmalloc 等窗口的单一事实来源（single source of truth）。

use crate::boot::{BootInfo, KernelVaLayout};
use crate::config;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

const DEFAULT_VA_BITS: u8 = 48;
const DEFAULT_PAGE_LEVELS: u8 = 4;
const DEFAULT_VMEMMAP_START: u64 = config::VMEMMAP_START;
const DEFAULT_VMEMMAP_END: u64 = config::VMEMMAP_END;
const DEFAULT_MODULES_START: u64 = config::MODULES_START;
const DEFAULT_MODULES_END: u64 = config::MODULES_END;
const DEFAULT_FIXMAP_START: u64 = config::FIXMAP_START;
const DEFAULT_FIXMAP_END: u64 = config::FIXMAP_END;

static VA_BITS: AtomicU8 = AtomicU8::new(DEFAULT_VA_BITS);
static PAGE_LEVELS: AtomicU8 = AtomicU8::new(DEFAULT_PAGE_LEVELS);
static DIRECT_MAP_START: AtomicU64 = AtomicU64::new(config::DIRECT_MAP_OFFSET);
static DIRECT_MAP_END: AtomicU64 = AtomicU64::new(config::VMALLOC_START);
static VMALLOC_START: AtomicU64 = AtomicU64::new(config::VMALLOC_START);
static VMALLOC_END: AtomicU64 = AtomicU64::new(config::VMALLOC_END.saturating_add(1));
static VMEMMAP_START: AtomicU64 = AtomicU64::new(DEFAULT_VMEMMAP_START);
static VMEMMAP_END: AtomicU64 = AtomicU64::new(DEFAULT_VMEMMAP_END.saturating_add(1));
static MODULES_START: AtomicU64 = AtomicU64::new(DEFAULT_MODULES_START);
static MODULES_END: AtomicU64 = AtomicU64::new(DEFAULT_MODULES_END.saturating_add(1));
static FIXMAP_START: AtomicU64 = AtomicU64::new(DEFAULT_FIXMAP_START);
static FIXMAP_END: AtomicU64 = AtomicU64::new(DEFAULT_FIXMAP_END.saturating_add(1));
static BOOT_REPORTED_VA_BITS: AtomicU8 = AtomicU8::new(DEFAULT_VA_BITS);
static BOOT_REPORTED_PAGE_LEVELS: AtomicU8 = AtomicU8::new(DEFAULT_PAGE_LEVELS);
static BOOT_REPORTED_ROOT: AtomicU64 = AtomicU64::new(0);
static HW_VA_BITS: AtomicU8 = AtomicU8::new(DEFAULT_VA_BITS);
static HW_PAGE_LEVELS: AtomicU8 = AtomicU8::new(DEFAULT_PAGE_LEVELS);
static HW_ROOT: AtomicU64 = AtomicU64::new(0);
static PAGING_CORRECTED_BY_HW: AtomicBool = AtomicBool::new(false);
static PAGING_ROOT_MISMATCH: AtomicBool = AtomicBool::new(false);

fn valid_window(start: u64, end: u64) -> bool {
    start < end
}

fn validate_layout(layout: &KernelVaLayout) -> bool {
    if layout.va_bits != 48 && layout.va_bits != 57 {
        return false;
    }
    if layout.page_levels != 4 && layout.page_levels != 5 {
        return false;
    }
    if !valid_window(layout.direct_map_start, layout.direct_map_end)
        || !valid_window(layout.vmalloc_start, layout.vmalloc_end)
        || !valid_window(layout.vmemmap_start, layout.vmemmap_end)
        || !valid_window(layout.modules_start, layout.modules_end)
        || !valid_window(layout.fixmap_start, layout.fixmap_end)
    {
        return false;
    }
    if layout.direct_map_end > layout.vmalloc_start {
        return false;
    }
    if layout.vmalloc_end > layout.vmemmap_start {
        return false;
    }
    if layout.vmemmap_end > layout.modules_start {
        return false;
    }
    if layout.modules_end > layout.fixmap_start {
        return false;
    }
    true
}

#[inline]
fn apply_layout(layout: &KernelVaLayout) {
    VA_BITS.store(layout.va_bits, Ordering::Release);
    PAGE_LEVELS.store(layout.page_levels, Ordering::Release);
    DIRECT_MAP_START.store(layout.direct_map_start, Ordering::Release);
    DIRECT_MAP_END.store(layout.direct_map_end, Ordering::Release);
    VMALLOC_START.store(layout.vmalloc_start, Ordering::Release);
    VMALLOC_END.store(layout.vmalloc_end, Ordering::Release);
    VMEMMAP_START.store(layout.vmemmap_start, Ordering::Release);
    VMEMMAP_END.store(layout.vmemmap_end, Ordering::Release);
    MODULES_START.store(layout.modules_start, Ordering::Release);
    MODULES_END.store(layout.modules_end, Ordering::Release);
    FIXMAP_START.store(layout.fixmap_start, Ordering::Release);
    FIXMAP_END.store(layout.fixmap_end, Ordering::Release);
}

pub fn snapshot() -> KernelVaLayout {
    KernelVaLayout {
        va_bits: VA_BITS.load(Ordering::Acquire),
        page_levels: PAGE_LEVELS.load(Ordering::Acquire),
        _reserved0: [0; 6],
        direct_map_start: DIRECT_MAP_START.load(Ordering::Acquire),
        direct_map_end: DIRECT_MAP_END.load(Ordering::Acquire),
        vmalloc_start: VMALLOC_START.load(Ordering::Acquire),
        vmalloc_end: VMALLOC_END.load(Ordering::Acquire),
        vmemmap_start: VMEMMAP_START.load(Ordering::Acquire),
        vmemmap_end: VMEMMAP_END.load(Ordering::Acquire),
        modules_start: MODULES_START.load(Ordering::Acquire),
        modules_end: MODULES_END.load(Ordering::Acquire),
        fixmap_start: FIXMAP_START.load(Ordering::Acquire),
        fixmap_end: FIXMAP_END.load(Ordering::Acquire),
    }
}

pub fn init_from_boot_info(info: &BootInfo) -> bool {
    let mut layout = info.kernel_layout;
    if layout.direct_map_start == 0 || layout.direct_map_end == 0 {
        layout = KernelVaLayout {
            va_bits: DEFAULT_VA_BITS,
            page_levels: DEFAULT_PAGE_LEVELS,
            _reserved0: [0; 6],
            direct_map_start: info.direct_map_offset,
            direct_map_end: config::VMALLOC_START,
            vmalloc_start: config::VMALLOC_START,
            vmalloc_end: config::VMALLOC_END.saturating_add(1),
            vmemmap_start: DEFAULT_VMEMMAP_START,
            vmemmap_end: DEFAULT_VMEMMAP_END.saturating_add(1),
            modules_start: DEFAULT_MODULES_START,
            modules_end: DEFAULT_MODULES_END.saturating_add(1),
            fixmap_start: DEFAULT_FIXMAP_START,
            fixmap_end: DEFAULT_FIXMAP_END.saturating_add(1),
        };
    }

    if !validate_layout(&layout) {
        return false;
    }

    BOOT_REPORTED_VA_BITS.store(layout.va_bits, Ordering::Release);
    BOOT_REPORTED_PAGE_LEVELS.store(layout.page_levels, Ordering::Release);
    BOOT_REPORTED_ROOT.store(info.page_table_root_phys(), Ordering::Release);
    PAGING_CORRECTED_BY_HW.store(false, Ordering::Release);
    PAGING_ROOT_MISMATCH.store(false, Ordering::Release);
    apply_layout(&layout);

    // 启动切换 trampoline 可能回退到 4-level；以硬件实际状态为准修正运行时视图。
    let hw = crate::mm::arch::paging_hardware_state();
    HW_VA_BITS.store(hw.va_bits, Ordering::Release);
    HW_PAGE_LEVELS.store(hw.page_levels, Ordering::Release);
    HW_ROOT.store(hw.cr3_root, Ordering::Release);
    if hw.va_bits != layout.va_bits || hw.page_levels != layout.page_levels {
        let mut corrected = layout;
        corrected.va_bits = hw.va_bits;
        corrected.page_levels = hw.page_levels;
        apply_layout(&corrected);
        PAGING_CORRECTED_BY_HW.store(true, Ordering::Release);
        crate::warn!(
            "BootInfo paging layout mismatch corrected by hardware state: boot va_bits={} levels={}, runtime va_bits={} levels={} cr3={:#x} cr4={:#x}",
            layout.va_bits,
            layout.page_levels,
            hw.va_bits,
            hw.page_levels,
            hw.cr3_root,
            hw.cr4
        );
    }
    if info.page_table_root_phys() != 0 && info.page_table_root_phys() != hw.cr3_root {
        PAGING_ROOT_MISMATCH.store(true, Ordering::Release);
        crate::warn!(
            "BootInfo root table mismatch with hardware CR3: boot_root={:#x}, hw_root={:#x} (possible LA57 trampoline fallback or boot-time root switch)",
            info.page_table_root_phys(),
            hw.cr3_root
        );
    }
    true
}

#[inline]
pub fn va_bits() -> u8 {
    VA_BITS.load(Ordering::Acquire)
}

#[inline]
pub fn page_levels() -> u8 {
    PAGE_LEVELS.load(Ordering::Acquire)
}

#[inline]
pub fn direct_map_offset() -> u64 {
    DIRECT_MAP_START.load(Ordering::Acquire)
}

#[inline]
pub fn direct_map_end() -> u64 {
    DIRECT_MAP_END.load(Ordering::Acquire)
}

#[inline]
pub fn vmalloc_start() -> u64 {
    VMALLOC_START.load(Ordering::Acquire)
}

#[inline]
pub fn vmalloc_end() -> u64 {
    VMALLOC_END.load(Ordering::Acquire)
}

#[inline]
pub fn direct_map_phys_to_virt(phys: u64) -> u64 {
    phys.saturating_add(direct_map_offset())
}

#[inline]
pub fn direct_map_virt_to_phys(virt: u64) -> Option<u64> {
    virt.checked_sub(direct_map_offset())
}

#[inline]
pub fn is_direct_map_addr(addr: u64) -> bool {
    let start = direct_map_offset();
    let end = direct_map_end();
    addr >= start && addr < end
}

#[inline]
pub fn is_vmalloc_addr(addr: u64) -> bool {
    let start = vmalloc_start();
    let end = vmalloc_end();
    addr >= start && addr < end
}

#[inline]
pub fn boot_reported_va_bits() -> u8 {
    BOOT_REPORTED_VA_BITS.load(Ordering::Acquire)
}

#[inline]
pub fn boot_reported_page_levels() -> u8 {
    BOOT_REPORTED_PAGE_LEVELS.load(Ordering::Acquire)
}

#[inline]
pub fn hardware_va_bits() -> u8 {
    HW_VA_BITS.load(Ordering::Acquire)
}

#[inline]
pub fn hardware_page_levels() -> u8 {
    HW_PAGE_LEVELS.load(Ordering::Acquire)
}

#[inline]
pub fn paging_corrected_by_hw() -> bool {
    PAGING_CORRECTED_BY_HW.load(Ordering::Acquire)
}

#[inline]
pub fn boot_reported_root_phys() -> u64 {
    BOOT_REPORTED_ROOT.load(Ordering::Acquire)
}

#[inline]
pub fn hardware_root_phys() -> u64 {
    HW_ROOT.load(Ordering::Acquire)
}

#[inline]
pub fn paging_root_mismatch() -> bool {
    PAGING_ROOT_MISMATCH.load(Ordering::Acquire)
}
