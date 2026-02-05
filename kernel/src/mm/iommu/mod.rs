// ============================================================================
// january_os - IOMMU (I/O Memory Management Unit) 支持
//
// 实现 Intel VT-d 和软件 SWIOTLB，提供 DMA 地址映射
// ============================================================================

mod vtd;
mod swiotlb;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::config;

pub use vtd::{VtdUnit, VtdCapability};
pub use swiotlb::Swiotlb;

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
static mut IOMMU_MANAGER: IommuManager = IommuManager::new();

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
    let mgr = unsafe { &mut *core::ptr::addr_of_mut!(IOMMU_MANAGER) };
    
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
        init_swiotlb(mgr);
        mgr.initialized.store(true, Ordering::SeqCst);
        return;
    }
    
    // 尝试初始化 Intel VT-d
    if try_init_vtd(mgr, direct_map_offset) {
        mgr.iommu_type = IommuType::IntelVtd;
        mgr.enabled.store(true, Ordering::SeqCst);
        mgr.initialized.store(true, Ordering::SeqCst);
        return;
    }
    
    // 尝试初始化 AMD-Vi (未实现)
    // if try_init_amdvi(mgr) { ... }
    
    // 无硬件 IOMMU，使用 SWIOTLB
    init_swiotlb(mgr);
    mgr.initialized.store(true, Ordering::SeqCst);
}

/// 检查 IOMMU 是否启用
pub fn enabled() -> bool {
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
    mgr.enabled.load(Ordering::Relaxed)
}

/// 获取 IOMMU 类型
pub fn iommu_type() -> IommuType {
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
    mgr.iommu_type
}

/// 获取翻译模式
pub fn translation_mode() -> TranslationMode {
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
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
pub fn map(phys_addr: u64, size: usize, _dir: DmaDirection) -> DmaAddr {
    let mgr = unsafe { &mut *core::ptr::addr_of_mut!(IOMMU_MANAGER) };
    
    if !mgr.enabled.load(Ordering::Relaxed) {
        // 无 IOMMU，直接返回物理地址
        return DmaAddr::new(phys_addr);
    }
    
    match mgr.iommu_type {
        IommuType::IntelVtd => {
            vtd_map(mgr, phys_addr, size)
        }
        IommuType::Swiotlb => {
            if let Some(ref mut swiotlb) = mgr.swiotlb {
                swiotlb.map(phys_addr, size)
            } else {
                DmaAddr::new(phys_addr)
            }
        }
        _ => DmaAddr::new(phys_addr),
    }
}

/// 取消 DMA 映射
pub fn unmap(dma_addr: DmaAddr, size: usize, _dir: DmaDirection) {
    let mgr = unsafe { &mut *core::ptr::addr_of_mut!(IOMMU_MANAGER) };
    
    if !mgr.enabled.load(Ordering::Relaxed) {
        return;
    }
    
    match mgr.iommu_type {
        IommuType::IntelVtd => {
            vtd_unmap(mgr, dma_addr, size);
        }
        IommuType::Swiotlb => {
            if let Some(ref mut swiotlb) = mgr.swiotlb {
                swiotlb.unmap(dma_addr, size);
            }
        }
        _ => {}
    }
}

/// 获取统计信息
pub fn stats() -> IommuStats {
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
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

// ============================================================================
// 内部函数
// ============================================================================

/// 尝试初始化 Intel VT-d
fn try_init_vtd(mgr: &mut IommuManager, direct_map_offset: u64) -> bool {
    // 从 ACPI DMAR 表获取 VT-d 信息
    let dmar = match crate::acpi::find_table::<crate::acpi::Dmar>() {
        Some(d) => d,
        None => return false,
    };
    
    let dmar_info = crate::acpi::parse_dmar(dmar);
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

/// 初始化 SWIOTLB
fn init_swiotlb(mgr: &mut IommuManager) {
    mgr.swiotlb = Some(Swiotlb::new(config::SWIOTLB_SIZE as usize));
    mgr.iommu_type = IommuType::Swiotlb;
}

/// VT-d 映射
fn vtd_map(mgr: &mut IommuManager, phys_addr: u64, size: usize) -> DmaAddr {
    // 在 Passthrough 模式下，DMA 地址 == 物理地址
    if mgr.translation_mode == TranslationMode::Passthrough {
        return DmaAddr::new(phys_addr);
    }
    
    // Translate 模式：使用第一个可用的 VT-d 单元进行映射
    for i in 0..mgr.nr_vtd_units {
        if let Some(ref mut unit) = mgr.vtd_units[i] {
            if let Some(dma_addr) = unit.map_pages(phys_addr, size) {
                let pages = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
                mgr.mapped_pages.fetch_add(pages as u64, Ordering::Relaxed);
                return dma_addr;
            }
        }
    }
    
    // 映射失败，返回物理地址（可能导致 DMA 错误）
    DmaAddr::new(phys_addr)
}

/// VT-d 取消映射
fn vtd_unmap(mgr: &mut IommuManager, dma_addr: DmaAddr, size: usize) {
    if mgr.translation_mode == TranslationMode::Passthrough {
        return;
    }
    
    for i in 0..mgr.nr_vtd_units {
        if let Some(ref mut unit) = mgr.vtd_units[i] {
            unit.unmap_pages(dma_addr, size);
        }
    }
    
    let pages = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    mgr.mapped_pages.fetch_sub(pages as u64, Ordering::Relaxed);
}

// ============================================================================
// 兼容旧 API
// ============================================================================

/// SWIOTLB 大小常量
pub const SWIOTLB_SIZE: u64 = config::SWIOTLB_SIZE;

pub fn init_iommu() {
    init(config::DIRECT_MAP_OFFSET);
}

pub fn iommu_initialized() -> bool {
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
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

use super::zone::GfpFlags;
use super::buddy::alloc_pages;
use super::page::page_to_pfn;

/// 分配 DMA 一致性内存
pub fn dma_alloc_coherent(size: usize, gfp: GfpFlags) -> Option<(*mut u8, DmaAddr)> {
    if size == 0 {
        return None;
    }
    
    let pages = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    let order = pages.next_power_of_two().trailing_zeros() as usize;
    
    let page = alloc_pages(order, gfp)?;
    let phys = page_to_pfn(page) * PAGE_SIZE;
    
    let dma_addr = map(phys, size, DmaDirection::Bidirectional);
    
    if dma_addr.is_null() {
        unsafe { super::buddy::free_pages(page, order) };
        return None;
    }
    
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
    let virt = (phys + mgr.direct_map_offset) as *mut u8;
    
    Some((virt, dma_addr))
}

/// 释放 DMA 一致性内存
pub fn dma_free_coherent(virt: *mut u8, dma_addr: DmaAddr, size: usize) {
    if virt.is_null() || size == 0 {
        return;
    }
    
    unmap(dma_addr, size, DmaDirection::Bidirectional);
    
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
    let phys = (virt as u64).saturating_sub(mgr.direct_map_offset);
    let pfn = phys / PAGE_SIZE;
    
    unsafe {
        let page = super::page::pfn_to_page(pfn);
        let pages = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        let order = pages.next_power_of_two().trailing_zeros() as usize;
        super::buddy::free_pages(page, order);
    }
}

/// 映射单个缓冲区用于 DMA
pub fn dma_map_single(virt: *const u8, size: usize, dir: DmaDirection) -> DmaAddr {
    if virt.is_null() || size == 0 {
        return DmaAddr::NULL;
    }
    
    let mgr = unsafe { &*core::ptr::addr_of!(IOMMU_MANAGER) };
    let phys = (virt as u64).saturating_sub(mgr.direct_map_offset);
    
    map(phys, size, dir)
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
