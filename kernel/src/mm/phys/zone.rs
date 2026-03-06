// ============================================================================
// january_os - Zone 内存分区管理
//
// 参考 Linux 内核，将物理内存划分为不同的 Zone
// ============================================================================

use super::page::{ListHead, Page, PageFlags};
use crate::config;
use crate::mm::vm::layout::PAGE_SIZE;
use crate::sync::{IrqSpinLock, IrqSpinLockGuard};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ============================================================================
// 常量定义 (从配置导入)
// ============================================================================

/// Buddy System 最大 order (2^MAX_ORDER 页 = 最大块)
pub const MAX_ORDER: usize = config::BUDDY_MAX_ORDER;

/// Zone 类型数量
pub const NR_ZONES: usize = 3;

/// ZONE_DMA 上限 - 传统 ISA DMA 设备 (从配置导入)
pub const ZONE_DMA_LIMIT: u64 = config::ZONE_DMA_LIMIT;

/// ZONE_DMA32 上限 - 32位 PCI 设备 DMA (从配置导入)
pub const ZONE_DMA32_LIMIT: u64 = config::ZONE_DMA32_LIMIT;

static ZONE_AREA_UNDERFLOW_REJECTS: AtomicU64 = AtomicU64::new(0);
static ZONE_GLOBAL_UNDERFLOW_REJECTS: AtomicU64 = AtomicU64::new(0);
static ZONE_SNAPSHOT_MISMATCHES: AtomicU64 = AtomicU64::new(0);
static ZONE_SCRUB_REPAIRS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub struct ZoneGuardStats {
    pub area_underflow_rejects: u64,
    pub global_underflow_rejects: u64,
    pub snapshot_mismatches: u64,
    pub scrub_repairs: u64,
}

#[inline]
pub fn zone_guard_stats() -> ZoneGuardStats {
    ZoneGuardStats {
        area_underflow_rejects: ZONE_AREA_UNDERFLOW_REJECTS.load(Ordering::Relaxed),
        global_underflow_rejects: ZONE_GLOBAL_UNDERFLOW_REJECTS.load(Ordering::Relaxed),
        snapshot_mismatches: ZONE_SNAPSHOT_MISMATCHES.load(Ordering::Relaxed),
        scrub_repairs: ZONE_SCRUB_REPAIRS.load(Ordering::Relaxed),
    }
}

// ============================================================================
// Zone 类型
// ============================================================================

/// 内存区域类型
///
/// Zone 划分：
/// - ZONE_DMA: 0-16MB，传统 ISA DMA 设备
/// - ZONE_DMA32: 16MB-4GB，32位 PCI 设备（无 IOMMU 时）
/// - ZONE_NORMAL: 4GB以上，大部分内核分配
///
/// 注：启用 IOMMU 后，32位设备可通过 IOMMU 映射访问高地址内存，
/// 届时 ZONE_DMA32 的重要性会降低。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ZoneType {
    /// DMA 区域 (0-16MB) - ISA DMA 兼容设备
    Dma = 0,
    /// DMA32 区域 (16MB-4GB) - 32位 PCI 设备
    Dma32 = 1,
    /// 普通区域 (4GB以上) - 大部分内核和用户分配
    Normal = 2,
}

impl ZoneType {
    /// 从物理地址确定 Zone 类型
    #[inline]
    pub fn from_phys_addr(addr: u64) -> Self {
        if addr < ZONE_DMA_LIMIT {
            ZoneType::Dma
        } else if addr < ZONE_DMA32_LIMIT {
            ZoneType::Dma32
        } else {
            ZoneType::Normal
        }
    }

    /// 获取 Zone 名称
    pub fn name(&self) -> &'static str {
        match self {
            ZoneType::Dma => "DMA",
            ZoneType::Dma32 => "DMA32",
            ZoneType::Normal => "Normal",
        }
    }

    /// 迭代所有 Zone 类型
    pub fn iter() -> impl Iterator<Item = ZoneType> {
        [ZoneType::Dma, ZoneType::Dma32, ZoneType::Normal].into_iter()
    }
}

// ============================================================================
// GFP Flags (Get Free Pages)
// ============================================================================

/// 内存分配标志
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct GfpFlags(u32);

impl GfpFlags {
    // ========== Zone 选择 ==========
    /// 从 ZONE_DMA 分配 (低 16MB)
    pub const DMA: u32 = 1 << 0;
    /// 从 ZONE_DMA32 分配 (低 4GB) - 32位设备 DMA
    pub const DMA32: u32 = 1 << 1;
    /// 从 ZONE_NORMAL 分配（默认）
    pub const NORMAL: u32 = 1 << 2;

    // ========== 行为修饰 ==========
    /// 清零分配的内存
    pub const ZERO: u32 = 1 << 8;
    /// 原子上下文，不能睡眠
    pub const ATOMIC: u32 = 1 << 9;
    /// 内核分配
    pub const KERNEL: u32 = 1 << 10;
    /// 用户空间分配
    pub const USER: u32 = 1 << 11;
    /// 不等待，立即返回
    pub const NOWAIT: u32 = 1 << 12;
    /// 失败后重试
    pub const RETRY: u32 = 1 << 13;
    /// 复合页
    pub const COMP: u32 = 1 << 14;
    /// 高优先级
    pub const HIGH: u32 = 1 << 15;

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(&self) -> u32 {
        self.0
    }

    pub fn test(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }

    pub fn set(&mut self, flag: u32) {
        self.0 |= flag;
    }
}

// ========== 常用 GFP 组合 ==========

/// 正常内核分配
pub const GFP_KERNEL: GfpFlags = GfpFlags::new(GfpFlags::KERNEL | GfpFlags::NORMAL);
/// 原子上下文分配（中断处理等）
pub const GFP_ATOMIC: GfpFlags =
    GfpFlags::new(GfpFlags::ATOMIC | GfpFlags::NORMAL | GfpFlags::HIGH);
/// 用户空间分配
pub const GFP_USER: GfpFlags = GfpFlags::new(GfpFlags::USER | GfpFlags::NORMAL);
/// DMA 内存 (ISA, 低 16MB)
pub const GFP_DMA: GfpFlags = GfpFlags::new(GfpFlags::DMA);
/// DMA32 内存 (32位设备, 低 4GB)
pub const GFP_DMA32: GfpFlags = GfpFlags::new(GfpFlags::DMA32);
/// 清零的内核内存
pub const GFP_KERNEL_ZERO: GfpFlags =
    GfpFlags::new(GfpFlags::KERNEL | GfpFlags::NORMAL | GfpFlags::ZERO);

// ============================================================================
// Free Area (Buddy 空闲区域)
// ============================================================================

/// Buddy 空闲区域
///
/// 每个 order 一个，管理该 order 的空闲块链表
#[repr(C)]
pub struct FreeArea {
    /// 空闲块双向链表头
    pub free_list: ListHead,
    /// 空闲块数量
    pub nr_free: u64,
}

impl FreeArea {
    pub const fn new() -> Self {
        Self {
            free_list: ListHead::new(),
            nr_free: 0,
        }
    }

    pub fn init(&mut self) {
        self.free_list.init();
        self.nr_free = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.nr_free == 0
    }
}

impl Default for FreeArea {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Zone 结构
// ============================================================================

/// 内存区域
///
/// UMA (Uniform Memory Access) 设计 - 单一内存节点
/// 未来可扩展为 NUMA 支持
pub struct Zone {
    /// Zone 类型
    pub zone_type: ZoneType,
    /// Zone 名称
    pub name: &'static str,
    /// 起始 PFN (Page Frame Number)
    pub start_pfn: u64,
    /// 页帧数量（包含空洞）
    pub spanned_pages: u64,
    /// 实际存在的页帧数量
    pub present_pages: u64,
    /// 空闲页帧数量
    pub free_pages: AtomicU64,
    /// Buddy 空闲区域 [order]
    pub free_area: [FreeArea; MAX_ORDER],
    /// Zone 锁 - 保护 free_area 链表操作
    pub lock: IrqSpinLock<()>,
    /// 保留的页帧数（低水位）
    pub watermark_min: u64,
    /// 低水位
    pub watermark_low: u64,
    /// 高水位
    pub watermark_high: u64,
    /// 是否已初始化
    pub initialized: bool,
}

// Zone 内部包含用于页链表的裸指针（ListHead），这些指针由外部锁协议保护。
// 通过 `IrqSpinLock<Zone>` 暴露时需要 `Zone: Send`，这里显式声明。
unsafe impl Send for Zone {}

impl Zone {
    pub const fn uninit() -> Self {
        Self {
            zone_type: ZoneType::Normal,
            name: "",
            start_pfn: 0,
            spanned_pages: 0,
            present_pages: 0,
            free_pages: AtomicU64::new(0),
            free_area: [
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
                FreeArea::new(),
            ],
            lock: IrqSpinLock::new(()),
            watermark_min: 0,
            watermark_low: 0,
            watermark_high: 0,
            initialized: false,
        }
    }

    /// 初始化 Zone
    pub fn init(&mut self, zone_type: ZoneType, start_pfn: u64, page_count: u64) {
        self.zone_type = zone_type;
        self.name = zone_type.name();
        self.start_pfn = start_pfn;
        self.spanned_pages = page_count;
        self.present_pages = page_count;
        self.free_pages.store(0, Ordering::Relaxed);

        for area in self.free_area.iter_mut() {
            area.init();
        }

        // 设置水位线（简单策略）
        self.watermark_min = page_count / 64; // ~1.5%
        self.watermark_low = page_count / 32; // ~3%
        self.watermark_high = page_count / 16; // ~6%

        self.initialized = true;
    }

    /// 获取空闲页数
    #[inline]
    pub fn nr_free_pages(&self) -> u64 {
        self.free_pages.load(Ordering::Relaxed)
    }

    fn recompute_free_pages_locked(&self) -> u64 {
        let mut recomputed = 0u64;
        for order in 0..MAX_ORDER {
            let blocks = self.free_area[order].nr_free;
            let pages_per_block = 1u64 << order;
            let add = blocks.saturating_mul(pages_per_block);
            recomputed = recomputed.saturating_add(add);
        }
        recomputed
    }

    /// 在持有 Zone 锁的前提下，返回 (observed, recomputed)。
    pub fn free_pages_snapshot_locked(&self) -> (u64, u64) {
        let observed = self.free_pages.load(Ordering::Relaxed);
        let recomputed = self.recompute_free_pages_locked();
        if observed != recomputed {
            ZONE_SNAPSHOT_MISMATCHES.fetch_add(1, Ordering::Relaxed);
        }
        (observed, recomputed)
    }

    /// 在持有 Zone 锁的前提下，仅按 free_area 计算空闲页总数（只读）。
    pub fn recomputed_free_pages_locked(&self) -> u64 {
        self.recompute_free_pages_locked()
    }

    /// 在持有 Zone 锁的前提下，将 free_pages 原子值纠偏到 recomputed 值。
    /// 返回 (observed, recomputed, repaired)。
    pub fn scrub_free_pages_locked(&mut self) -> (u64, u64, bool) {
        let observed = self.free_pages.load(Ordering::Relaxed);
        let recomputed = self.recompute_free_pages_locked();
        if observed != recomputed {
            ZONE_SCRUB_REPAIRS.fetch_add(1, Ordering::Relaxed);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[zone] scrub free_pages: zone={} observed={} recomputed={}",
                    self.name,
                    observed,
                    recomputed
                );
            }
            self.free_pages.store(recomputed, Ordering::Relaxed);
            return (observed, recomputed, true);
        }
        (observed, recomputed, false)
    }

    /// 结束 PFN
    #[inline]
    pub fn end_pfn(&self) -> u64 {
        self.start_pfn + self.spanned_pages
    }

    /// 检查 PFN 是否属于此 Zone
    #[inline]
    pub fn contains_pfn(&self, pfn: u64) -> bool {
        pfn >= self.start_pfn && pfn < self.end_pfn()
    }

    /// 检查是否低于最小水位
    pub fn is_low_on_memory(&self) -> bool {
        self.nr_free_pages() < self.watermark_min
    }

    /// 添加空闲块到 Buddy 系统
    pub unsafe fn add_to_buddy(&mut self, page: &mut Page, order: usize) {
        debug_assert!(order < MAX_ORDER, "Order too large");

        page.mark_buddy(order as u8);

        let area = &mut self.free_area[order];
        area.free_list.add(&mut page.lru as *mut ListHead);
        area.nr_free += 1;

        let pages = 1u64 << order;
        self.free_pages.fetch_add(pages, Ordering::Relaxed);
    }

    /// 从 Buddy 系统移除空闲块
    pub unsafe fn remove_from_buddy(&mut self, page: &mut Page, order: usize) -> bool {
        debug_assert!(order < MAX_ORDER, "Order too large");
        debug_assert!(page.is_buddy(), "Page not in buddy");

        let pages = 1u64 << order;
        let area = &mut self.free_area[order];
        if area.nr_free == 0 {
            ZONE_AREA_UNDERFLOW_REJECTS.fetch_add(1, Ordering::Relaxed);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[zone] remove_from_buddy underflow: zone={} order={} page_ptr={:#x}",
                    self.name,
                    order,
                    page as *mut Page as usize,
                );
            }
            return false;
        }

        page.lru.del();
        area.nr_free -= 1;

        page.clear_buddy();

        let free_before = self.free_pages.load(Ordering::Relaxed);
        if free_before < pages {
            ZONE_GLOBAL_UNDERFLOW_REJECTS.fetch_add(1, Ordering::Relaxed);
            self.free_pages.store(0, Ordering::Relaxed);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[zone] free_pages underflow clamp: zone={} order={} free_before={} pages={}",
                    self.name,
                    order,
                    free_before,
                    pages,
                );
            }
            return true;
        }

        self.free_pages.fetch_sub(pages, Ordering::Relaxed);
        true
    }
}

impl Default for Zone {
    fn default() -> Self {
        Self::uninit()
    }
}

// ============================================================================
// 全局 Zone 数组 (UMA - 单节点)
// ============================================================================

/// 全局 Zone 数组
pub static ZONES: [IrqSpinLock<Zone>; NR_ZONES] = [
    IrqSpinLock::new(Zone::uninit()),
    IrqSpinLock::new(Zone::uninit()),
    IrqSpinLock::new(Zone::uninit()),
];

/// 获取 Zone 引用
pub fn get_zone(zone_type: ZoneType) -> IrqSpinLockGuard<'static, Zone> {
    ZONES[zone_type as usize].lock()
}

/// 根据 PFN 获取 Zone
pub fn pfn_to_zone(pfn: u64) -> Option<IrqSpinLockGuard<'static, Zone>> {
    let phys = pfn * PAGE_SIZE;
    let zone_type = ZoneType::from_phys_addr(phys);
    let zone = get_zone(zone_type);
    if zone.initialized && zone.contains_pfn(pfn) {
        Some(zone)
    } else {
        None
    }
}

/// 根据 GFP flags 选择合适的 Zone 列表
///
/// 返回优先级从高到低的 Zone 列表
/// 分配时按顺序尝试，直到成功
pub fn gfp_to_zone_list(gfp: GfpFlags) -> &'static [ZoneType] {
    if gfp.test(GfpFlags::DMA) {
        // DMA: 只能从 ZONE_DMA
        &[ZoneType::Dma]
    } else if gfp.test(GfpFlags::DMA32) {
        // DMA32: 优先 DMA32，回退到 DMA
        &[ZoneType::Dma32, ZoneType::Dma]
    } else {
        // Normal: 优先 Normal，回退到 DMA32，再回退到 DMA
        &[ZoneType::Normal, ZoneType::Dma32, ZoneType::Dma]
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 计算 order 对应的页数
#[inline]
pub const fn pages_per_order(order: usize) -> u64 {
    1u64 << order
}

/// 计算 order 对应的字节数
#[inline]
pub const fn bytes_per_order(order: usize) -> u64 {
    (1u64 << order) * PAGE_SIZE
}

/// 计算能容纳指定页数的最小 order
#[inline]
pub fn get_order(pages: u64) -> usize {
    if pages == 0 {
        return 0;
    }
    let pages = pages.next_power_of_two();
    pages.trailing_zeros() as usize
}

/// 计算伙伴块的 PFN
#[inline]
pub fn get_buddy_pfn(pfn: u64, order: usize) -> u64 {
    pfn ^ (1u64 << order)
}

// ============================================================================
// 初始化状态
// ============================================================================

/// Zone 子系统是否已初始化
static ZONES_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// 检查 Zone 是否已初始化
pub fn zones_initialized() -> bool {
    ZONES_INITIALIZED.load(Ordering::Acquire)
}

/// 标记 Zone 已初始化
pub fn mark_zones_initialized() {
    ZONES_INITIALIZED.store(true, Ordering::Release);
}
