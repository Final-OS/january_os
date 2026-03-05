// ============================================================================
// january_os - IOMMU (I/O Memory Management Unit) 支持
//
// 实现 Intel VT-d 和软件 SWIOTLB，提供 DMA 地址映射
// ============================================================================

mod swiotlb;
mod vtd;

use crate::config;
use crate::sync::SpinLock;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub use swiotlb::Swiotlb;
pub use vtd::{VtdCapability, VtdUnit};

// ============================================================================
// 常量
// ============================================================================

/// 最大 IOMMU 单元数量
pub const MAX_IOMMU_UNITS: usize = 8;

/// DMA 地址空间大小 (4GB for 32-bit devices)
pub const DMA_ADDR_SIZE: u64 = 4 * 1024 * 1024 * 1024;

/// 页大小
pub const PAGE_SIZE: u64 = 4096;

// ============================================================================
// 类型定义
// ============================================================================

/// IOMMU 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuType {
    None,
    IntelVtd,
    AmdVi,
    Swiotlb,
}

/// 地址翻译模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationMode {
    /// 1:1 映射 (DMA addr == phys addr)
    Passthrough,
    /// 完整地址翻译
    Translate,
}

/// DMA 方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DmaDirection {
    Bidirectional = 0,
    ToDevice = 1,
    FromDevice = 2,
    None = 3,
}

/// DMA 地址
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct DmaAddr(pub u64);

impl DmaAddr {
    pub const NULL: DmaAddr = DmaAddr(0);

    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

// ============================================================================
// 全局状态
// ============================================================================

/// IOMMU 管理器
static IOMMU_MANAGER: SpinLock<IommuManager> =
    SpinLock::with_name(IommuManager::new(), "IommuManager");
static DMA_COHERENT_TRACKER: SpinLock<DmaCoherentTracker> =
    SpinLock::with_name(DmaCoherentTracker::new(), "DmaCoherentTracker");

const DMA_COHERENT_TRACK_CAP: usize = 256;
static DMA_COHERENT_TRACK_INSERT_FAIL: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_INVALID_VIRT: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_META_MISS: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_SIZE_MISMATCH: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_DMA_MISMATCH: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_PFN_OOB: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_OWNER_MISMATCH: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_ORDER_MISMATCH: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_IN_PROGRESS_CONFLICT: AtomicU64 = AtomicU64::new(0);
static DMA_COHERENT_FREE_ROLLBACK: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Default)]
pub struct DmaCoherentGuardStats {
    pub track_insert_fail: u64,
    pub free_invalid_virt: u64,
    pub free_meta_miss: u64,
    pub free_size_mismatch: u64,
    pub free_dma_mismatch: u64,
    pub free_pfn_oob: u64,
    pub free_owner_mismatch: u64,
    pub free_order_mismatch: u64,
    pub free_in_progress_conflict: u64,
    pub free_rollback: u64,
}

pub fn dma_coherent_guard_stats() -> DmaCoherentGuardStats {
    DmaCoherentGuardStats {
        track_insert_fail: DMA_COHERENT_TRACK_INSERT_FAIL.load(Ordering::Relaxed),
        free_invalid_virt: DMA_COHERENT_FREE_INVALID_VIRT.load(Ordering::Relaxed),
        free_meta_miss: DMA_COHERENT_FREE_META_MISS.load(Ordering::Relaxed),
        free_size_mismatch: DMA_COHERENT_FREE_SIZE_MISMATCH.load(Ordering::Relaxed),
        free_dma_mismatch: DMA_COHERENT_FREE_DMA_MISMATCH.load(Ordering::Relaxed),
        free_pfn_oob: DMA_COHERENT_FREE_PFN_OOB.load(Ordering::Relaxed),
        free_owner_mismatch: DMA_COHERENT_FREE_OWNER_MISMATCH.load(Ordering::Relaxed),
        free_order_mismatch: DMA_COHERENT_FREE_ORDER_MISMATCH.load(Ordering::Relaxed),
        free_in_progress_conflict: DMA_COHERENT_FREE_IN_PROGRESS_CONFLICT.load(Ordering::Relaxed),
        free_rollback: DMA_COHERENT_FREE_ROLLBACK.load(Ordering::Relaxed),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum DmaTrackState {
    Empty = 0,
    Live = 1,
    Freeing = 2,
}

#[derive(Clone, Copy)]
struct DmaCoherentMeta {
    state: u8,
    virt: u64,
    dma: u64,
    size: usize,
    pfn: u64,
    order: usize,
}

impl DmaCoherentMeta {
    const fn empty() -> Self {
        Self {
            state: DmaTrackState::Empty as u8,
            virt: 0,
            dma: 0,
            size: 0,
            pfn: 0,
            order: 0,
        }
    }

    fn state(&self) -> DmaTrackState {
        match self.state {
            1 => DmaTrackState::Live,
            2 => DmaTrackState::Freeing,
            _ => DmaTrackState::Empty,
        }
    }

    fn set_state(&mut self, state: DmaTrackState) {
        self.state = state as u8;
    }
}

struct DmaCoherentTracker {
    entries: [DmaCoherentMeta; DMA_COHERENT_TRACK_CAP],
}

impl DmaCoherentTracker {
    const fn new() -> Self {
        Self {
            entries: [DmaCoherentMeta::empty(); DMA_COHERENT_TRACK_CAP],
        }
    }

    fn insert(&mut self, meta: DmaCoherentMeta) -> bool {
        for slot in self.entries.iter_mut() {
            if slot.state() == DmaTrackState::Empty {
                *slot = meta;
                slot.set_state(DmaTrackState::Live);
                return true;
            }
        }
        false
    }

    fn find_by_virt(&self, virt: u64) -> Option<usize> {
        self.entries
            .iter()
            .position(|slot| slot.state() != DmaTrackState::Empty && slot.virt == virt)
    }
}

fn rollback_coherent_free(meta: DmaCoherentMeta) {
    let mut tracker = DMA_COHERENT_TRACKER.lock();
    if let Some(idx) = tracker.find_by_virt(meta.virt) {
        let slot = &mut tracker.entries[idx];
        if slot.state() == DmaTrackState::Freeing && slot.dma == meta.dma {
            *slot = meta;
            slot.set_state(DmaTrackState::Live);
            DMA_COHERENT_FREE_ROLLBACK.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn commit_coherent_free(meta: DmaCoherentMeta) {
    let mut tracker = DMA_COHERENT_TRACKER.lock();
    if let Some(idx) = tracker.find_by_virt(meta.virt) {
        let slot = &mut tracker.entries[idx];
        if slot.state() == DmaTrackState::Freeing && slot.dma == meta.dma {
            *slot = DmaCoherentMeta::empty();
        }
    }
}

/// IOMMU 管理器
pub struct IommuManager {
    /// 已初始化
    initialized: AtomicBool,
    /// 启用状态
    enabled: AtomicBool,
    /// IOMMU 类型
    iommu_type: IommuType,
    /// 翻译模式
    translation_mode: TranslationMode,
    /// VT-d 单元数量
    nr_vtd_units: usize,
    /// VT-d 单元
    vtd_units: [Option<VtdUnit>; MAX_IOMMU_UNITS],
    /// SWIOTLB (后备)
    swiotlb: Option<Swiotlb>,
    /// 直接映射偏移
    direct_map_offset: u64,
    /// 统计：已映射页数
    mapped_pages: AtomicU64,
}

impl IommuManager {
    const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            iommu_type: IommuType::None,
            translation_mode: TranslationMode::Passthrough,
            nr_vtd_units: 0,
            vtd_units: [None, None, None, None, None, None, None, None],
            swiotlb: None,
            direct_map_offset: 0,
            mapped_pages: AtomicU64::new(0),
        }
    }
}

// ============================================================================
// 公共 API
// ============================================================================

/// 初始化 IOMMU 子系统
pub fn init(direct_map_offset: u64) {
    let mut mgr = IOMMU_MANAGER.lock();

    if mgr.initialized.load(Ordering::Relaxed) {
        return;
    }

    mgr.direct_map_offset = direct_map_offset;

    // 确定翻译模式
    mgr.translation_mode = if config::IOMMU_PASSTHROUGH {
        TranslationMode::Passthrough
    } else {
        TranslationMode::Translate
    };

    // 检查配置
    if !config::IOMMU_ENABLED && !config::IOMMU_AUTO_DETECT {
        // IOMMU 禁用，使用 SWIOTLB
        init_swiotlb(&mut mgr);
        mgr.initialized.store(true, Ordering::SeqCst);
        return;
    }

    // 尝试初始化 Intel VT-d
    if try_init_vtd(&mut mgr, direct_map_offset) {
        mgr.iommu_type = IommuType::IntelVtd;
        mgr.enabled.store(true, Ordering::SeqCst);
        mgr.initialized.store(true, Ordering::SeqCst);
        return;
    }

    // 尝试探测 AMD-Vi（当前尚无硬件后端，探测到后降级到 SWIOTLB）
    let _ = try_probe_amdvi();

    // 无硬件 IOMMU，使用 SWIOTLB
    init_swiotlb(&mut mgr);
    mgr.initialized.store(true, Ordering::SeqCst);
}

/// 检查 IOMMU 是否启用
pub fn enabled() -> bool {
    let mgr = IOMMU_MANAGER.lock();
    mgr.enabled.load(Ordering::Relaxed)
}

/// 获取 IOMMU 类型
pub fn iommu_type() -> IommuType {
    let mgr = IOMMU_MANAGER.lock();
    mgr.iommu_type
}

/// 获取翻译模式
pub fn translation_mode() -> TranslationMode {
    let mgr = IOMMU_MANAGER.lock();
    mgr.translation_mode
}

/// 映射物理地址用于 DMA
///
/// # Arguments
/// * `phys_addr` - 物理地址
/// * `size` - 大小
/// * `dir` - DMA 方向
///
/// # Returns
/// DMA 地址
pub fn map(phys_addr: u64, size: usize, dir: DmaDirection) -> DmaAddr {
    let mut mgr = IOMMU_MANAGER.lock();

    if !mgr.enabled.load(Ordering::Relaxed) {
        // 无 IOMMU，直接返回物理地址
        return DmaAddr::new(phys_addr);
    }

    match mgr.iommu_type {
        IommuType::IntelVtd => vtd_map(&mut mgr, phys_addr, size),
        IommuType::Swiotlb => {
            if let Some(ref mut swiotlb) = mgr.swiotlb {
                swiotlb.map(phys_addr, size, dir)
            } else {
                DmaAddr::NULL
            }
        }
        _ => DmaAddr::NULL,
    }
}

/// 取消 DMA 映射
pub fn unmap(dma_addr: DmaAddr, size: usize, dir: DmaDirection) {
    let mut mgr = IOMMU_MANAGER.lock();

    if !mgr.enabled.load(Ordering::Relaxed) {
        return;
    }

    match mgr.iommu_type {
        IommuType::IntelVtd => {
            vtd_unmap(&mut mgr, dma_addr, size);
        }
        IommuType::Swiotlb => {
            if let Some(ref mut swiotlb) = mgr.swiotlb {
                swiotlb.unmap(dma_addr, size, dir);
            }
        }
        _ => {}
    }
}

/// 获取统计信息
pub fn stats() -> IommuStats {
    let mgr = IOMMU_MANAGER.lock();
    IommuStats {
        enabled: mgr.enabled.load(Ordering::Relaxed),
        iommu_type: mgr.iommu_type,
        translation_mode: mgr.translation_mode,
        nr_units: mgr.nr_vtd_units,
        mapped_pages: mgr.mapped_pages.load(Ordering::Relaxed),
    }
}

/// IOMMU 统计信息
pub struct IommuStats {
    pub enabled: bool,
    pub iommu_type: IommuType,
    pub translation_mode: TranslationMode,
    pub nr_units: usize,
    pub mapped_pages: u64,
}

#[inline]
fn size_to_pages(size: usize) -> Option<usize> {
    if size == 0 {
        return None;
    }

    let page_size = PAGE_SIZE as usize;
    size.checked_add(page_size - 1)
        .map(|rounded| rounded / page_size)
        .filter(|pages| *pages != 0)
}

#[inline]
fn pages_to_order(pages: usize) -> Option<usize> {
    if pages == 0 {
        return None;
    }

    let rounded = pages.checked_next_power_of_two()?;
    Some(rounded.trailing_zeros() as usize)
}

#[inline]
fn mapped_pages_add(counter: &AtomicU64, pages: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(pages))
    });
}

#[inline]
fn mapped_pages_sub(counter: &AtomicU64, pages: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_sub(pages))
    });
}

// ============================================================================
// 内部函数
// ============================================================================

/// 尝试初始化 Intel VT-d
fn try_init_vtd(mgr: &mut IommuManager, direct_map_offset: u64) -> bool {
    // 从 ACPI DMAR 表获取 VT-d 信息
    let dmar = match crate::drivers::acpi::find_table::<crate::drivers::acpi::Dmar>() {
        Some(d) => d,
        None => return false,
    };

    let dmar_info = crate::drivers::acpi::parse_dmar(dmar);
    if dmar_info.drhd_count == 0 {
        return false;
    }

    // 初始化每个 DRHD 单元
    for i in 0..dmar_info.drhd_count.min(MAX_IOMMU_UNITS) {
        let drhd = &dmar_info.drhds[i];

        let mut unit = VtdUnit::new(drhd.register_base, direct_map_offset);

        // 初始化 VT-d 单元
        if let Err(_e) = unit.init(mgr.translation_mode) {
            continue;
        }

        mgr.vtd_units[i] = Some(unit);
        mgr.nr_vtd_units += 1;
    }

    mgr.nr_vtd_units > 0
}

/// 探测 AMD-Vi（IVRS）并报告当前支持状态
fn try_probe_amdvi() -> bool {
    if !crate::drivers::acpi::has_table(b"IVRS") {
        return false;
    }

    crate::warn!(
        "[IOMMU] ACPI IVRS detected, but AMD-Vi backend is not implemented; falling back to SWIOTLB"
    );
    false
}

/// 初始化 SWIOTLB
fn init_swiotlb(mgr: &mut IommuManager) {
    mgr.swiotlb = Some(Swiotlb::new(
        config::SWIOTLB_SIZE as usize,
        mgr.direct_map_offset,
    ));
    mgr.iommu_type = IommuType::Swiotlb;
    // SWIOTLB 属于软件地址翻译后端，需要走 map/unmap 路径
    mgr.enabled.store(true, Ordering::SeqCst);
}

/// VT-d 映射
fn vtd_map(mgr: &mut IommuManager, phys_addr: u64, size: usize) -> DmaAddr {
    // 在 Passthrough 模式下，DMA 地址 == 物理地址
    if mgr.translation_mode == TranslationMode::Passthrough {
        return DmaAddr::new(phys_addr);
    }

    let pages = match size_to_pages(size) {
        Some(p) => p,
        None => {
            crate::warn!("[IOMMU] invalid DMA map size {}", size);
            return DmaAddr::NULL;
        }
    };

    // Translate 模式：使用第一个可用的 VT-d 单元进行映射
    for i in 0..mgr.nr_vtd_units {
        if let Some(ref mut unit) = mgr.vtd_units[i] {
            if let Some(dma_addr) = unit.map_pages(phys_addr, size) {
                mapped_pages_add(&mgr.mapped_pages, pages as u64);
                return dma_addr;
            }
        }
    }

    crate::warn!(
        "[IOMMU] VT-d map failed: phys={:#x} size={} mode={:?}",
        phys_addr,
        size,
        mgr.translation_mode
    );
    DmaAddr::NULL
}

/// VT-d 取消映射
fn vtd_unmap(mgr: &mut IommuManager, dma_addr: DmaAddr, size: usize) {
    if mgr.translation_mode == TranslationMode::Passthrough {
        return;
    }

    let pages = match size_to_pages(size) {
        Some(p) => p,
        None => {
            crate::warn!("[IOMMU] invalid DMA unmap size {}", size);
            return;
        }
    };

    for i in 0..mgr.nr_vtd_units {
        if let Some(ref mut unit) = mgr.vtd_units[i] {
            unit.unmap_pages(dma_addr, size);
        }
    }

    mapped_pages_sub(&mgr.mapped_pages, pages as u64);
}

// ============================================================================
// 兼容旧 API
// ============================================================================

/// SWIOTLB 大小常量
pub const SWIOTLB_SIZE: u64 = config::SWIOTLB_SIZE;

pub fn init_iommu() {
    init(crate::mm::direct_map_offset());
}

pub fn iommu_initialized() -> bool {
    let mgr = IOMMU_MANAGER.lock();
    mgr.initialized.load(Ordering::Relaxed)
}

pub fn iommu_enabled() -> bool {
    enabled()
}

pub fn iommu_config_enabled() -> bool {
    config::IOMMU_ENABLED
}

pub fn iommu_config_auto() -> bool {
    config::IOMMU_AUTO_DETECT
}

pub fn iommu_stats() -> IommuStats {
    stats()
}

// ============================================================================
// DMA 一致性内存 API
// ============================================================================

use super::page::{PageOwner, max_pfn, page_to_pfn, pfn_to_page};
use crate::mm::page::buddy::alloc_pages;
use crate::mm::page::zone::GfpFlags;

/// 分配 DMA 一致性内存
pub fn dma_alloc_coherent(size: usize, gfp: GfpFlags) -> Option<(*mut u8, DmaAddr)> {
    let pages = size_to_pages(size)?;
    let order = pages_to_order(pages)?;

    let page = alloc_pages(order, gfp)?;
    let phys = match page_to_pfn(page).checked_mul(PAGE_SIZE) {
        Some(p) => p,
        None => {
            unsafe { crate::mm::page::buddy::free_pages(page, order) };
            crate::warn!("[IOMMU] dma_alloc_coherent phys overflow, order={}", order);
            return None;
        }
    };

    let dma_addr = map(phys, size, DmaDirection::Bidirectional);

    if dma_addr.is_null() {
        unsafe { crate::mm::page::buddy::free_pages(page, order) };
        return None;
    }

    let direct_map_offset = {
        let mgr = IOMMU_MANAGER.lock();
        mgr.direct_map_offset
    };

    let virt = match phys.checked_add(direct_map_offset) {
        Some(v) => v as *mut u8,
        None => {
            crate::warn!("[IOMMU] dma_alloc_coherent virt overflow");
            unmap(dma_addr, size, DmaDirection::Bidirectional);
            unsafe { crate::mm::page::buddy::free_pages(page, order) };
            return None;
        }
    };

    let meta = DmaCoherentMeta {
        state: DmaTrackState::Live as u8,
        virt: virt as u64,
        dma: dma_addr.as_u64(),
        size,
        pfn: page_to_pfn(page),
        order,
    };
    let inserted = {
        let mut tracker = DMA_COHERENT_TRACKER.lock();
        tracker.insert(meta)
    };
    if !inserted {
        DMA_COHERENT_TRACK_INSERT_FAIL.fetch_add(1, Ordering::Relaxed);
        crate::warn!(
            "[IOMMU] dma_alloc_coherent tracker full (cap={})",
            DMA_COHERENT_TRACK_CAP
        );
        unmap(dma_addr, size, DmaDirection::Bidirectional);
        unsafe { crate::mm::page::buddy::free_pages(page, order) };
        return None;
    }

    Some((virt, dma_addr))
}

/// 释放 DMA 一致性内存
pub fn dma_free_coherent(virt: *mut u8, dma_addr: DmaAddr, size: usize) {
    if virt.is_null() || size == 0 {
        return;
    }

    let direct_map_offset = {
        let mgr = IOMMU_MANAGER.lock();
        mgr.direct_map_offset
    };

    let virt_addr = virt as u64;
    if virt_addr < direct_map_offset {
        DMA_COHERENT_FREE_INVALID_VIRT.fetch_add(1, Ordering::Relaxed);
        crate::warn!(
            "[IOMMU] dma_free_coherent invalid virt={:#x}, direct_map_offset={:#x}",
            virt_addr,
            direct_map_offset
        );
        return;
    }

    let tracked = {
        let mut tracker = DMA_COHERENT_TRACKER.lock();
        let idx = match tracker.find_by_virt(virt_addr) {
            Some(i) => i,
            None => {
                DMA_COHERENT_FREE_META_MISS.fetch_add(1, Ordering::Relaxed);
                crate::warn!(
                    "[IOMMU] dma_free_coherent metadata miss virt={:#x}",
                    virt_addr
                );
                return;
            }
        };

        if tracker.entries[idx].state() == DmaTrackState::Freeing {
            DMA_COHERENT_FREE_IN_PROGRESS_CONFLICT.fetch_add(1, Ordering::Relaxed);
            crate::warn!(
                "[IOMMU] dma_free_coherent in-progress conflict virt={:#x}",
                virt_addr
            );
            return;
        }
        if tracker.entries[idx].size != size {
            DMA_COHERENT_FREE_SIZE_MISMATCH.fetch_add(1, Ordering::Relaxed);
            crate::warn!(
                "[IOMMU] dma_free_coherent size mismatch virt={:#x} expected={} actual={}",
                virt_addr,
                tracker.entries[idx].size,
                size
            );
            return;
        }
        if tracker.entries[idx].dma != dma_addr.as_u64() {
            DMA_COHERENT_FREE_DMA_MISMATCH.fetch_add(1, Ordering::Relaxed);
            crate::warn!(
                "[IOMMU] dma_free_coherent dma mismatch virt={:#x} expected={:#x} actual={:#x}",
                virt_addr,
                tracker.entries[idx].dma,
                dma_addr.as_u64()
            );
            return;
        }

        let mut meta = tracker.entries[idx];
        meta.set_state(DmaTrackState::Freeing);
        tracker.entries[idx].set_state(DmaTrackState::Freeing);
        meta
    };

    let pages = match size_to_pages(tracked.size) {
        Some(p) => p,
        None => {
            rollback_coherent_free(tracked);
            return;
        }
    };
    let order = match pages_to_order(pages) {
        Some(o) => o,
        None => {
            rollback_coherent_free(tracked);
            return;
        }
    };
    if order != tracked.order {
        DMA_COHERENT_FREE_ORDER_MISMATCH.fetch_add(1, Ordering::Relaxed);
        crate::warn!(
            "[IOMMU] dma_free_coherent order mismatch virt={:#x} tracked={} derived={}",
            virt_addr,
            tracked.order,
            order
        );
        rollback_coherent_free(tracked);
        return;
    }
    if tracked.pfn >= max_pfn() {
        DMA_COHERENT_FREE_PFN_OOB.fetch_add(1, Ordering::Relaxed);
        crate::warn!(
            "[IOMMU] dma_free_coherent pfn out of range virt={:#x} pfn={}",
            virt_addr,
            tracked.pfn
        );
        rollback_coherent_free(tracked);
        return;
    }

    unmap(
        DmaAddr::new(tracked.dma),
        tracked.size,
        DmaDirection::Bidirectional,
    );

    unsafe {
        let page = &mut *pfn_to_page(tracked.pfn);
        let owner = page.owner();
        if owner != PageOwner::Allocated && owner != PageOwner::Unknown {
            DMA_COHERENT_FREE_OWNER_MISMATCH.fetch_add(1, Ordering::Relaxed);
            crate::warn!(
                "[IOMMU] dma_free_coherent owner mismatch virt={:#x} pfn={} owner={:?}",
                virt_addr,
                tracked.pfn,
                owner
            );
        }
        crate::mm::page::buddy::free_pages(page, tracked.order);
    }
    commit_coherent_free(tracked);
}

/// 映射单个缓冲区用于 DMA
pub fn dma_map_single(virt: *const u8, size: usize, dir: DmaDirection) -> DmaAddr {
    if virt.is_null() || size == 0 {
        return DmaAddr::NULL;
    }

    let direct_map_offset = {
        let mgr = IOMMU_MANAGER.lock();
        mgr.direct_map_offset
    };

    let virt_addr = virt as u64;
    if virt_addr < direct_map_offset {
        crate::warn!(
            "[IOMMU] dma_map_single invalid virt={:#x}, direct_map_offset={:#x}",
            virt_addr,
            direct_map_offset
        );
        return DmaAddr::NULL;
    }

    let phys = virt_addr - direct_map_offset;
    let dma = map(phys, size, dir);
    if dma.is_null() {
        crate::warn!(
            "[IOMMU] dma_map_single failed: virt={:#x} phys={:#x} size={} dir={:?}",
            virt_addr,
            phys,
            size,
            dir
        );
    }
    dma
}

/// 取消单个缓冲区的 DMA 映射
pub fn dma_unmap_single(dma_addr: DmaAddr, size: usize, dir: DmaDirection) {
    unmap(dma_addr, size, dir);
}

/// 同步 DMA 缓冲区 (CPU -> Device)
pub fn dma_sync_single_for_device(_dma_addr: DmaAddr, _size: usize, _dir: DmaDirection) {
    // x86 是缓存一致的，不需要显式同步
}

/// 同步 DMA 缓冲区 (Device -> CPU)
pub fn dma_sync_single_for_cpu(_dma_addr: DmaAddr, _size: usize, _dir: DmaDirection) {
    // x86 是缓存一致的，不需要显式同步
}
