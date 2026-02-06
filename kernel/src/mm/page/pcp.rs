// ============================================================================
// january_os - PCP (Per-CPU Page Cache) 每 CPU 页缓存
//
// 减少 Zone 锁竞争，加速单页分配/释放
// ============================================================================

use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use crate::interrupt::apic::local_apic_id;
use super::page::{Page, ListHead};
use super::zone::{Zone, ZoneType, GfpFlags, get_zone, NR_ZONES};
use crate::config;
use crate::sync::SpinLock;

/// container_of 宏
macro_rules! container_of {
    ($ptr:expr, $type:ty, $field:ident) => {{
        let ptr = $ptr as *const u8;
        let offset = core::mem::offset_of!($type, $field);
        unsafe { ptr.sub(offset) as *mut $type }
    }};
}

// ============================================================================
// 常量 (从配置导入)
// ============================================================================

/// 最大 CPU 数量
pub const MAX_CPUS: usize = config::MAX_CPUS;

/// PCP 高水位默认值
const PCP_HIGH_DEFAULT: u32 = config::PCP_HIGH_WATERMARK;

/// PCP 批量操作数量默认值
const PCP_BATCH_DEFAULT: u32 = config::PCP_BATCH_SIZE;

/// 每个 Zone 类型一个 PCP
pub const NR_PCP_LISTS: usize = NR_ZONES;

// ============================================================================
// PCP 数据结构
// ============================================================================

/// PCP 内部状态 (受 SpinLock 保护)
pub struct PcpInner {
    /// 缓存的页数
    pub count: u32,
    /// 空闲页链表头
    pub list: ListHead,
}

impl PcpInner {
    pub const fn new() -> Self {
        Self {
            count: 0,
            list: ListHead::new(),
        }
    }
    
    pub fn init(&mut self) {
        self.count = 0;
        self.list.init();
    }
}

/// Per-CPU 页缓存
pub struct PerCpuPages {
    /// 高水位 (超过则归还)
    pub high: u32,
    /// 批量操作数量
    pub batch: u32,
    /// 受保护的内部状态
    inner: SpinLock<PcpInner>,
}

impl PerCpuPages {
    pub const fn new() -> Self {
        Self {
            high: PCP_HIGH_DEFAULT,
            batch: PCP_BATCH_DEFAULT,
            inner: SpinLock::new(PcpInner::new()),
        }
    }
    
    pub fn init(&mut self) {
        self.high = PCP_HIGH_DEFAULT;
        self.batch = PCP_BATCH_DEFAULT;
        self.inner.lock().init();
    }
    
    /// 获取缓存的页数
    #[inline]
    pub fn nr_pages(&self) -> u32 {
        self.inner.lock().count
    }
    
    /// 是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nr_pages() == 0
    }
    
    /// 是否超过高水位
    #[inline]
    pub fn above_high(&self) -> bool {
        self.nr_pages() > self.high
    }
    
    /// 从 PCP 分配一页
    pub fn alloc(&self) -> Option<&'static mut Page> {
        let mut inner = self.inner.lock();
        
        if inner.count > 0 {
            unsafe {
                // 从链表头取一个
                let node = inner.list.next;
                if node != &inner.list as *const _ as *mut _ {
                    (*node).del();
                    inner.count -= 1;
                    
                    let page = container_of!(node, Page, lru);
                    return Some(&mut *page);
                }
            }
        }
        
        None
    }
    
    /// 归还一页到 PCP
    pub fn free(&self, page: &mut Page) {
        let mut inner = self.inner.lock();
        
        unsafe {
            // 添加到链表头
            inner.list.add(&mut page.lru);
            inner.count += 1;
        }
    }
    
    /// 批量从 Buddy 补充
    pub unsafe fn refill_from_buddy(&self, zone: &mut Zone, batch: u32) -> u32 {
        let mut inner = self.inner.lock();
        let mut added = 0u32;
        
        while added < batch {
            // 从 Buddy order-0 分配
            if zone.free_area[0].nr_free == 0 {
                break;
            }
            
            let node = zone.free_area[0].free_list.next;
            if node == &zone.free_area[0].free_list as *const _ as *mut _ {
                break;
            }
            
            let page = container_of!(node, Page, lru);
            zone.remove_from_buddy(&mut *page, 0);
            
            // 添加到 PCP
            inner.list.add(&mut (*page).lru);
            inner.count += 1;
            added += 1;
        }
        
        added
    }
    
    /// 批量归还到 Buddy
    pub unsafe fn drain_to_buddy(&self, zone: &mut Zone, batch: u32) -> u32 {
        let mut inner = self.inner.lock();
        let mut drained = 0u32;
        
        while drained < batch && inner.count > 0 {
            let node = inner.list.next;
            if node == &inner.list as *const _ as *mut _ {
                break;
            }
            
            (*node).del();
            inner.count -= 1;
            
            let page = container_of!(node, Page, lru);
            zone.add_to_buddy(&mut *page, 0);
            drained += 1;
        }
        
        drained
    }
}

// ============================================================================
// Per-CPU 页集合
// ============================================================================

/// 一个 CPU 的所有 PCP
#[repr(C)]
pub struct PerCpuPageset {
    /// 每个 Zone 一个 PCP
    pub pcp: [PerCpuPages; NR_PCP_LISTS],
}

impl PerCpuPageset {
    pub const fn new() -> Self {
        Self {
            pcp: [
                PerCpuPages::new(),
                PerCpuPages::new(),
                PerCpuPages::new(),
            ],
        }
    }
    
    pub fn init(&mut self) {
        for pcp in self.pcp.iter_mut() {
            pcp.init();
        }
    }
}

// ============================================================================
// 全局 PCP 数组
// ============================================================================

/// 所有 CPU 的 PCP
static mut CPU_PAGESETS: [PerCpuPageset; MAX_CPUS] = {
    const UNINIT: PerCpuPageset = PerCpuPageset::new();
    [UNINIT; MAX_CPUS]
};

/// 当前 CPU 数量
static NR_CPUS: AtomicU32 = AtomicU32::new(1);

/// PCP 是否已初始化
static PCP_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 PCP 子系统
pub fn init_pcp(nr_cpus: u32) {
    let nr_cpus = nr_cpus.min(MAX_CPUS as u32);
    NR_CPUS.store(nr_cpus, Ordering::SeqCst);
    
    unsafe {
        for i in 0..nr_cpus as usize {
            CPU_PAGESETS[i].init();
        }
    }
    
    PCP_INITIALIZED.store(true, Ordering::SeqCst);
}

/// 检查 PCP 是否已初始化
pub fn pcp_initialized() -> bool {
    PCP_INITIALIZED.load(Ordering::Relaxed)
}

/// 获取当前 CPU ID
#[inline]
fn current_cpu() -> usize {
    let id = local_apic_id() as usize;
    if id < MAX_CPUS {
        id
    } else {
        0 // Fallback for invalid APIC ID
    }
}

// ============================================================================
// PCP 分配/释放接口
// ============================================================================

/// 从 PCP 分配单页
pub fn pcp_alloc_page(gfp: GfpFlags) -> Option<&'static mut Page> {
    if !pcp_initialized() {
        return None;
    }
    
    let cpu = current_cpu();
    let preferred_zone = gfp_to_zone_idx(gfp);
    
    // 尝试从 preferred zone 开始，依次向下回退
    // NORMAL -> DMA32 -> DMA
    for zone_idx in (0..=preferred_zone).rev() {
        unsafe {
            let pageset = &CPU_PAGESETS[cpu];
            let pcp = &pageset.pcp[zone_idx];
            
            // 尝试从 PCP 分配
            if let Some(page) = pcp.alloc() {
                return Some(page);
            }
            
            // PCP 为空，从 Buddy 补充
            let zone = get_zone(idx_to_zone_type(zone_idx));
            if zone.initialized && zone.nr_free_pages() > 0 {
                let batch = pcp.batch;
                pcp.refill_from_buddy(zone, batch);
                if let Some(page) = pcp.alloc() {
                    return Some(page);
                }
            }
        }
    }
    
    None
}

/// 释放单页到 PCP
pub fn pcp_free_page(page: &mut Page) {
    if !pcp_initialized() {
        return;
    }
    
    let cpu = current_cpu();
    let zone_idx = page.zone_id() as usize;
    
    if zone_idx >= NR_PCP_LISTS {
        return;
    }
    
    unsafe {
        let pageset = &CPU_PAGESETS[cpu];
        let pcp = &pageset.pcp[zone_idx];
        
        // 释放到 PCP
        pcp.free(page);
        
        // 如果超过高水位，批量归还给 Buddy
        if pcp.above_high() {
            let zone = get_zone(idx_to_zone_type(zone_idx));
            if zone.initialized {
                let batch = pcp.batch;
                pcp.drain_to_buddy(zone, batch);
            }
        }
    }
}

/// 排空所有 PCP (用于内存紧张时)
pub fn drain_all_pcps() {
    if !pcp_initialized() {
        return;
    }
    
    let nr_cpus = NR_CPUS.load(Ordering::Relaxed) as usize;
    
    unsafe {
        for cpu in 0..nr_cpus {
            let pageset = &CPU_PAGESETS[cpu];
            
            for zone_idx in 0..NR_PCP_LISTS {
                let pcp = &pageset.pcp[zone_idx];
                let zone = get_zone(idx_to_zone_type(zone_idx));
                
                if zone.initialized {
                    // 排空所有页
                    let batch = pcp.batch;
                    while pcp.nr_pages() > 0 {
                        pcp.drain_to_buddy(zone, batch);
                    }
                }
            }
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// GFP flags 转 Zone 索引
fn gfp_to_zone_idx(gfp: GfpFlags) -> usize {
    if gfp.test(GfpFlags::DMA) {
        0 // ZONE_DMA
    } else if gfp.test(GfpFlags::DMA32) {
        1 // ZONE_DMA32
    } else {
        2 // ZONE_NORMAL
    }
}

/// Zone 索引转 ZoneType
fn idx_to_zone_type(idx: usize) -> ZoneType {
    match idx {
        0 => ZoneType::Dma,
        1 => ZoneType::Dma32,
        _ => ZoneType::Normal,
    }
}

// ============================================================================
// 统计信息
// ============================================================================

/// PCP 统计信息
pub struct PcpStats {
    /// 总缓存页数
    pub total_cached: u64,
    /// 每个 Zone 的缓存页数
    pub per_zone: [u64; NR_PCP_LISTS],
}

/// 获取 PCP 统计信息
pub fn pcp_stats() -> PcpStats {
    let mut stats = PcpStats {
        total_cached: 0,
        per_zone: [0; NR_PCP_LISTS],
    };
    
    if !pcp_initialized() {
        return stats;
    }
    
    let nr_cpus = NR_CPUS.load(Ordering::Relaxed) as usize;
    
    unsafe {
        for cpu in 0..nr_cpus {
            let pageset = &CPU_PAGESETS[cpu];
            
            for zone_idx in 0..NR_PCP_LISTS {
                let count = pageset.pcp[zone_idx].nr_pages() as u64;
                stats.per_zone[zone_idx] += count;
                stats.total_cached += count;
            }
        }
    }
    
    stats
}
