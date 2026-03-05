// ============================================================================
// january_os - 内存管理初始化
//
// 定义内存管理系统的初始化流程和生命周期转换
// ============================================================================
//!
//! # 内存管理初始化阶段
//!
//! ```text
//! 阶段 0: 引导阶段 (Bootloader)
//!   - 建立恒等映射和直接映射
//!   - 收集 UEFI 内存映射
//!   - 跳转到内核
//!
//! 阶段 1: Memblock 初始化
//!   - 从 UEFI 内存映射构建 memblock
//!   - 使用 memblock 分配 struct page 数组
//!   - 使用 memblock 分配 Zone 结构
//!   - 分配的内存自动标记为 reserved
//!
//! 阶段 2: Buddy 初始化
//!   - 初始化各 Zone
//!   - 将 memblock 中的空闲内存添加到 Buddy 系统
//!   - Memblock 停止使用（但保留信息供调试）
//!
//! 阶段 3: SLUB 初始化
//!   - 初始化 kmalloc 缓存
//!   - 启用 kmalloc/kfree
//!
//! 阶段 4: 完全初始化
//!   - 内存管理系统完全就绪
//!   - 可使用所有分配接口
//! ```

use crate::mm::page::memblock::{
    memblock_init, memblock_initialized, memblock_add, memblock_reserve,
    memblock_alloc, memblock_for_each_free_region, memblock_phys_mem_size,
    memblock_reserved_region_count, memblock_reserved_region,
};
#[cfg(target_arch = "x86_64")]
use crate::mm::arch::{PageTable, PTE_ADDR_MASK, read_cr3};
use crate::mm::page::page::{Page, init_vmemmap, PAGE_STRUCT_SIZE};
use crate::mm::page::zone::{Zone, ZoneType, get_zone, mark_zones_initialized};
use crate::mm::page::buddy::init_zone_buddy;
use crate::mm::slub::init_kmalloc_caches;
use crate::mm::vm::layout::PAGE_SIZE;
use crate::error::{KernelError, KernelResult};
use core::sync::atomic::{AtomicU8, Ordering};

// ============================================================================
// 初始化阶段
// ============================================================================

/// 内存管理初始化阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmInitStage {
    /// 未初始化
    None = 0,
    /// Memblock 就绪（早期启动）
    Memblock = 1,
    /// Buddy 系统就绪
    Buddy = 2,
    /// SLUB 就绪
    Slub = 3,
    /// 完全初始化
    Complete = 4,
}

/// 当前初始化阶段
static INIT_STAGE: AtomicU8 = AtomicU8::new(MmInitStage::None as u8);

#[inline]
fn decode_stage(raw: u8) -> MmInitStage {
    match raw {
        x if x == MmInitStage::Memblock as u8 => MmInitStage::Memblock,
        x if x == MmInitStage::Buddy as u8 => MmInitStage::Buddy,
        x if x == MmInitStage::Slub as u8 => MmInitStage::Slub,
        x if x == MmInitStage::Complete as u8 => MmInitStage::Complete,
        _ => MmInitStage::None,
    }
}

/// 获取当前初始化阶段
pub fn init_stage() -> MmInitStage {
    decode_stage(INIT_STAGE.load(Ordering::Acquire))
}

/// 设置初始化阶段
unsafe fn set_stage(stage: MmInitStage) {
    INIT_STAGE.store(stage as u8, Ordering::Release);
}

// ============================================================================
// 内存区域信息（从 UEFI 传入）
// ============================================================================

/// 内存区域信息（从 UEFI 传入）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryRegionInfo {
    pub phys_start: u64,
    pub page_count: u64,
    pub is_usable: bool,
}

// ============================================================================
// Memblock 初始化
// ============================================================================

/// 初始化 memblock 子系统
/// 
/// # Arguments
/// * `regions` - 内存区域数组（从 UEFI 获取）
/// * `kernel_start` - 内核起始物理地址
/// * `kernel_end` - 内核结束物理地址
/// 
/// # Safety
/// 
/// 必须在内核早期调用，且只能调用一次
pub unsafe fn init_memblock(
    regions: &[MemoryRegionInfo],
    kernel_start: u64,
    kernel_end: u64,
) -> KernelResult<()> {
    if memblock_initialized() {
        return Err(KernelError::AlreadyExists);
    }

    // 初始化 memblock
    memblock_init();

    // 添加所有内存区域
    for region in regions {
        if region.is_usable {
            let size = region.page_count * PAGE_SIZE;
            memblock_add(region.phys_start, size)?;
        }
    }

    // 保留低端内存 (0 - 1MB)
    // 包含 BIOS、中断向量表等
    memblock_reserve(0, 0x100000)?;

    // 保留内核占用的内存
    let kernel_size = kernel_end - kernel_start;
    memblock_reserve(kernel_start, kernel_size)?;
    reserve_bootstrap_page_tables()?;

    set_stage(MmInitStage::Memblock);
    
    Ok(())
}

// ============================================================================
// Buddy 系统初始化
// ============================================================================

/// 初始化 Buddy 系统
/// 
/// # Arguments
/// * `max_pfn` - 最大 PFN
/// 
/// # Safety
/// 
/// - 必须在 Memblock 阶段之后调用
pub unsafe fn init_buddy_system(
    _regions: &[MemoryRegionInfo],
    max_pfn: u64,
    _direct_map_offset: u64,
) -> KernelResult<()> {
    unsafe {
        if init_stage() != MmInitStage::Memblock {
            return Err(KernelError::InvalidParam);
        }
        
        // 1. 使用 memblock 分配 struct page 数组
        let page_array_size = (max_pfn as usize) * PAGE_STRUCT_SIZE;
        let page_array_phys = memblock_alloc(page_array_size as u64, 64);
        if page_array_phys == 0 {
            return Err(KernelError::NoMemory);
        }
        
        // 转换为虚拟地址
        let page_array = phys_to_virt(page_array_phys) as *mut Page;
        
        // 初始化所有 Page 结构
        for i in 0..max_pfn {
            let page = &mut *page_array.add(i as usize);
            *page = Page::uninit();
        }
        
        init_vmemmap(page_array, max_pfn);

        // 2. 初始化所有 Page 元数据，并为 reserved PFN 预置保护属性
        for pfn in 0..max_pfn {
            let page = &mut *page_array.add(pfn as usize);
            let zone_type = ZoneType::from_phys_addr(pfn * PAGE_SIZE);
            page.init(zone_type as u8);
            if pfn_overlaps_reserved(pfn) {
                page.mark_reserved();
                page.set_count_one();
            }
        }
        
        // 3. 初始化 Zones
        let dma_end_pfn = (crate::mm::page::zone::ZONE_DMA_LIMIT / PAGE_SIZE).min(max_pfn);
        let dma32_end_pfn = (crate::mm::page::zone::ZONE_DMA32_LIMIT / PAGE_SIZE).min(max_pfn);
        
        // ZONE_DMA: 0 - 16MB (ISA DMA)
        if dma_end_pfn > 0 {
            let mut zone = get_zone(ZoneType::Dma);
            zone.init(ZoneType::Dma, 0, dma_end_pfn);
        }
        
        // ZONE_DMA32: 16MB - 4GB (32位 PCI 设备)
        if dma32_end_pfn > dma_end_pfn {
            let mut zone = get_zone(ZoneType::Dma32);
            zone.init(ZoneType::Dma32, dma_end_pfn, dma32_end_pfn - dma_end_pfn);
        }
        
        // ZONE_NORMAL: 4GB+ (普通内存)
        if max_pfn > dma32_end_pfn {
            let mut zone = get_zone(ZoneType::Normal);
            zone.init(ZoneType::Normal, dma32_end_pfn, max_pfn - dma32_end_pfn);
        }
        
        // 4. 遍历 memblock 中的空闲内存，添加到 Buddy 系统
        memblock_for_each_free_region(|base, size| {
            let start_pfn = base / PAGE_SIZE;
            let end_pfn = (base + size) / PAGE_SIZE;

            // 添加到对应 Zone 的 Buddy 系统
            // ZONE_DMA 部分
            if start_pfn < dma_end_pfn {
                let zone_start = start_pfn;
                let zone_end = end_pfn.min(dma_end_pfn);
                if zone_end > zone_start {
                    let mut zone = get_zone(ZoneType::Dma);
                    init_zone_buddy_filtered(&mut zone, zone_start, zone_end, max_pfn);
                }
            }
            
            // ZONE_DMA32 部分
            if start_pfn < dma32_end_pfn && end_pfn > dma_end_pfn {
                let zone_start = start_pfn.max(dma_end_pfn);
                let zone_end = end_pfn.min(dma32_end_pfn);
                if zone_end > zone_start {
                    let mut zone = get_zone(ZoneType::Dma32);
                    init_zone_buddy_filtered(&mut zone, zone_start, zone_end, max_pfn);
                }
            }
            
            // ZONE_NORMAL 部分
            if end_pfn > dma32_end_pfn {
                let zone_start = start_pfn.max(dma32_end_pfn);
                let zone_end = end_pfn.min(max_pfn);
                if zone_end > zone_start {
                    let mut zone = get_zone(ZoneType::Normal);
                    init_zone_buddy_filtered(&mut zone, zone_start, zone_end, max_pfn);
                }
            }
            
            true // 继续遍历
        });
        
        mark_zones_initialized();
        set_stage(MmInitStage::Buddy);
        
        Ok(())
    }
}

// ============================================================================
// SLUB 初始化
// ============================================================================

/// 初始化 SLUB 分配器
pub unsafe fn init_slub() -> KernelResult<()> {
    unsafe {
        if init_stage() != MmInitStage::Buddy {
            return Err(KernelError::InvalidParam);
        }
        
        init_kmalloc_caches();
        set_stage(MmInitStage::Slub);
        
        Ok(())
    }
}

/// 完成内存管理初始化
pub unsafe fn finish_mm_init() {
    unsafe {
        if init_stage() == MmInitStage::Slub {
            set_stage(MmInitStage::Complete);
        }
    }
}

// ============================================================================
// 统计信息
// ============================================================================

/// 获取 memblock 使用的物理内存总量
pub fn memblock_used_memory() -> u64 {
    memblock_phys_mem_size()
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 物理地址转虚拟地址（通过直接映射）
#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    crate::mm::phys_to_virt(phys)
}

#[inline]
fn pfn_overlaps_reserved(pfn: u64) -> bool {
    let base = pfn.saturating_mul(PAGE_SIZE);
    let end = base.saturating_add(PAGE_SIZE);

    for idx in 0..memblock_reserved_region_count() {
        let Some(region) = memblock_reserved_region(idx) else {
            continue;
        };
        if region.size == 0 {
            continue;
        }
        let region_end = region.base.saturating_add(region.size);
        if base < region_end && end > region.base {
            return true;
        }
    }

    false
}

unsafe fn init_zone_buddy_filtered(
    zone: &mut Zone,
    start_pfn: u64,
    end_pfn: u64,
    max_pfn: u64,
) {
    let end_pfn = end_pfn.min(max_pfn);
    if start_pfn >= end_pfn {
        return;
    }

    let mut run_start: Option<u64> = None;
    let mut pfn = start_pfn;

    while pfn < end_pfn {
        let reserved = pfn_overlaps_reserved(pfn);
        if reserved {
            if let Some(start) = run_start.take() {
                if pfn > start {
                    init_zone_buddy(zone, start, pfn);
                }
            }
        } else if run_start.is_none() {
            run_start = Some(pfn);
        }
        pfn += 1;
    }

    if let Some(start) = run_start {
        if end_pfn > start {
            init_zone_buddy(zone, start, end_pfn);
        }
    }
}

/// 打印初始化状态
pub fn print_mm_stats() {
    // 由调用者实现具体打印
}

#[cfg(target_arch = "x86_64")]
fn reserve_bootstrap_page_tables() -> KernelResult<()> {
    let root_phys = read_cr3() & PTE_ADDR_MASK;
    if root_phys == 0 {
        return Ok(());
    }
    let levels = crate::mm::page_levels().clamp(4, 5);
    reserve_bootstrap_table_level(root_phys, levels)
}

#[cfg(not(target_arch = "x86_64"))]
fn reserve_bootstrap_page_tables() -> KernelResult<()> {
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn reserve_bootstrap_table_level(table_phys: u64, level: u8) -> KernelResult<()> {
    memblock_reserve(table_phys, PAGE_SIZE)?;
    if level <= 1 {
        return Ok(());
    }

    let table = unsafe { &*(phys_to_virt(table_phys) as *const PageTable) };
    for entry in table.entries().iter() {
        if !entry.is_present() || entry.is_huge() {
            continue;
        }
        reserve_bootstrap_table_level(entry.phys_addr(), level - 1)?;
    }
    Ok(())
}
