// ============================================================================
// january_os - PCP (Per-CPU Page Cache) 每 CPU 页缓存
//
// 减少 Zone 锁竞争，加速单页分配/释放
// ============================================================================

use super::page::{max_pfn, page_to_pfn, vmemmap_base_ptr, ListHead, Page, PageOwner};
use super::zone::{get_zone, GfpFlags, Zone, ZoneType, NR_ZONES};
use crate::config;
use crate::interrupt::apic::local_apic_id;
use crate::sync::IrqSpinLock;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

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

/// PCP 存储容量上限（编译期）
const CPU_PAGESETS_CAPACITY: usize = config::MAX_CPUS;

/// PCP 高水位默认值
const PCP_HIGH_DEFAULT: u32 = config::PCP_HIGH_WATERMARK;

/// PCP 批量操作数量默认值
const PCP_BATCH_DEFAULT: u32 = config::PCP_BATCH_SIZE;

/// 每个 Zone 类型一个 PCP
pub const NR_PCP_LISTS: usize = NR_ZONES;

// ============================================================================
// PCP 数据结构
// ============================================================================

/// PCP 内部状态 (受 IrqSpinLock 保护)
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
    inner: IrqSpinLock<PcpInner>,
}

impl PerCpuPages {
    pub const fn new() -> Self {
        Self {
            high: PCP_HIGH_DEFAULT,
            batch: PCP_BATCH_DEFAULT,
            inner: IrqSpinLock::new(PcpInner::new()),
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
        // 获取 Zone 锁（锁序：PCP → Zone）
        let _zone_guard = (*core::ptr::addr_of!(zone.lock)).lock();
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
            if !zone.remove_from_buddy(&mut *page, 0) {
                break;
            }
            (*page).set_owner(PageOwner::Pcp);

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
        // 获取 Zone 锁（锁序：PCP → Zone）
        let _zone_guard = (*core::ptr::addr_of!(zone.lock)).lock();
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
            pcp: [PerCpuPages::new(), PerCpuPages::new(), PerCpuPages::new()],
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
struct CpuPagesets {
    inner: UnsafeCell<[PerCpuPageset; CPU_PAGESETS_CAPACITY]>,
}

unsafe impl Sync for CpuPagesets {}

impl CpuPagesets {
    const fn new() -> Self {
        const UNINIT: PerCpuPageset = PerCpuPageset::new();
        Self {
            inner: UnsafeCell::new([UNINIT; CPU_PAGESETS_CAPACITY]),
        }
    }
}

static CPU_PAGESETS: CpuPagesets = CpuPagesets::new();

/// 当前 CPU 数量
static NR_CPUS: AtomicU32 = AtomicU32::new(1);

/// PCP 是否已初始化
static PCP_INITIALIZED: AtomicBool = AtomicBool::new(false);
static PCP_INVALID_FREE_REJECTS: AtomicU64 = AtomicU64::new(0);
static PCP_QUARANTINE_FALLBACKS: AtomicU64 = AtomicU64::new(0);
static PCP_OWNER_MISMATCHES: AtomicU64 = AtomicU64::new(0);

/// 临时熔断开关
///
/// 历史上 PCP 快路径曾触发过页链表问题。当前实现已修复关键的 CPU 索引初始化边界，
/// 因此默认开启 PCP。
const PCP_TEMP_DISABLED: bool = false;

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 PCP 子系统
pub fn init_pcp(nr_cpus: u32) {
    let nr_cpus = nr_cpus.clamp(1, CPU_PAGESETS_CAPACITY as u32);
    NR_CPUS.store(nr_cpus, Ordering::SeqCst);

    unsafe {
        for i in 0..nr_cpus as usize {
            cpu_pageset_mut(i).init();
        }
    }

    PCP_INITIALIZED.store(true, Ordering::SeqCst);
}

/// 检查 PCP 是否已初始化
pub fn pcp_initialized() -> bool {
    if PCP_TEMP_DISABLED {
        return false;
    }
    PCP_INITIALIZED.load(Ordering::Relaxed)
}

#[inline]
fn nr_cpus() -> usize {
    NR_CPUS.load(Ordering::Relaxed) as usize
}

/// 获取当前 CPU ID
#[inline]
fn current_cpu() -> usize {
    let id = local_apic_id() as usize;
    if id < nr_cpus() {
        id
    } else {
        0 // Fallback for invalid APIC ID
    }
}

#[inline]
fn is_page_ptr_in_vmemmap(page: *const Page) -> bool {
    let base = vmemmap_base_ptr() as usize;
    let max = max_pfn() as usize;
    if base == 0 || max == 0 {
        return false;
    }
    let span_bytes = max.saturating_mul(core::mem::size_of::<Page>());
    let end = base.saturating_add(span_bytes);
    let ptr = page as usize;
    ptr >= base && ptr < end && ((ptr - base) % core::mem::size_of::<Page>() == 0)
}

#[inline]
unsafe fn fallback_to_buddy(page: &mut Page, zone_idx: usize) {
    let pfn = page_to_pfn(page);
    let mut zone = if let Some(zone) = super::zone::pfn_to_zone(pfn) {
        zone
    } else {
        if zone_idx >= NR_PCP_LISTS {
            PCP_INVALID_FREE_REJECTS.fetch_add(1, Ordering::Relaxed);
            return;
        }
        get_zone(idx_to_zone_type(zone_idx))
    };
    if !zone.initialized || !zone.contains_pfn(pfn) {
        PCP_INVALID_FREE_REJECTS.fetch_add(1, Ordering::Relaxed);
        return;
    }

    // 节点链指针异常时直接绕过 PCP，降级回收到 Buddy，避免静默泄漏。
    page.lru.next = core::ptr::null_mut();
    page.lru.prev = core::ptr::null_mut();
    page.clear_buddy();
    page.set_order(0);
    zone.add_to_buddy(page, 0);
    PCP_QUARANTINE_FALLBACKS.fetch_add(1, Ordering::Relaxed);
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
            let pageset = cpu_pageset(cpu);
            let pcp = &pageset.pcp[zone_idx];

            // 尝试从 PCP 分配
            if let Some(page) = pcp.alloc() {
                if !is_page_ptr_in_vmemmap(page as *const Page) {
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[pcp] invalid page pointer from pcp.alloc: cpu={} zone_idx={} page_ptr={:#x}",
                            cpu,
                            zone_idx,
                            page as *mut Page as usize,
                        );
                    }
                    continue;
                }
                let page_zone = page.zone_id() as usize;
                if page_zone != zone_idx {
                    PCP_OWNER_MISMATCHES.fetch_add(1, Ordering::Relaxed);
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[pcp] zone mismatch from pcp.alloc: cpu={} list_zone={} page_zone={} page_ptr={:#x} -> quarantine",
                            cpu,
                            zone_idx,
                            page_zone,
                            page as *mut Page as usize,
                        );
                    }
                    fallback_to_buddy(page, page_zone);
                    continue;
                }
                if page.owner() != PageOwner::Pcp {
                    PCP_OWNER_MISMATCHES.fetch_add(1, Ordering::Relaxed);
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[pcp] owner mismatch from pcp.alloc: cpu={} zone_idx={} owner={:?} page_ptr={:#x} -> buddy-fallback",
                            cpu,
                            zone_idx,
                            page.owner(),
                            page as *mut Page as usize,
                        );
                    }
                    fallback_to_buddy(page, page_zone);
                    continue;
                }
                page.set_owner(PageOwner::Allocated);
                return Some(page);
            }

            // PCP 为空，从 Buddy 补充
            let mut zone = get_zone(idx_to_zone_type(zone_idx));
            if zone.initialized && zone.nr_free_pages() > 0 {
                let batch = pcp.batch;
                pcp.refill_from_buddy(&mut zone, batch);
                if let Some(page) = pcp.alloc() {
                    if !is_page_ptr_in_vmemmap(page as *const Page) {
                        if crate::config::DEBUG_VERBOSE {
                            crate::kprintln!(
                                "\x1b[90m[diag]\x1b[0m[pcp] invalid page pointer after refill: cpu={} zone_idx={} page_ptr={:#x}",
                                cpu,
                                zone_idx,
                                page as *mut Page as usize,
                            );
                        }
                        continue;
                    }
                    let page_zone = page.zone_id() as usize;
                    if page_zone != zone_idx {
                        PCP_OWNER_MISMATCHES.fetch_add(1, Ordering::Relaxed);
                        if crate::config::DEBUG_VERBOSE {
                            crate::kprintln!(
                                "\x1b[90m[diag]\x1b[0m[pcp] zone mismatch after refill: cpu={} list_zone={} page_zone={} page_ptr={:#x} -> quarantine",
                                cpu,
                                zone_idx,
                                page_zone,
                                page as *mut Page as usize,
                            );
                        }
                        fallback_to_buddy(page, page_zone);
                        continue;
                    }
                    if page.owner() != PageOwner::Pcp {
                        PCP_OWNER_MISMATCHES.fetch_add(1, Ordering::Relaxed);
                        if crate::config::DEBUG_VERBOSE {
                            crate::kprintln!(
                                "\x1b[90m[diag]\x1b[0m[pcp] owner mismatch after refill: cpu={} zone_idx={} owner={:?} page_ptr={:#x} -> buddy-fallback",
                                cpu,
                                zone_idx,
                                page.owner(),
                                page as *mut Page as usize,
                            );
                        }
                        fallback_to_buddy(page, page_zone);
                        continue;
                    }
                    page.set_owner(PageOwner::Allocated);
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

    if !is_page_ptr_in_vmemmap(page as *const Page) {
        PCP_INVALID_FREE_REJECTS.fetch_add(1, Ordering::Relaxed);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pcp] reject invalid free page pointer: page_ptr={:#x}",
                page as *mut Page as usize,
            );
        }
        return;
    }

    let cpu = current_cpu();
    let zone_idx = page.zone_id() as usize;

    if zone_idx >= NR_PCP_LISTS {
        PCP_INVALID_FREE_REJECTS.fetch_add(1, Ordering::Relaxed);
        return;
    }

    unsafe {
        // 已在链表中的节点再次入链会破坏 PCP 链表，直接回退到 Buddy 避免污染扩散。
        if !page.lru.next.is_null() || !page.lru.prev.is_null() {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[pcp] suspicious lru links on free: cpu={} zone_idx={} page_ptr={:#x} lru_next={:#x} lru_prev={:#x} -> buddy-fallback",
                    cpu,
                    zone_idx,
                    page as *mut Page as usize,
                    page.lru.next as usize,
                    page.lru.prev as usize,
                );
            }
            fallback_to_buddy(page, zone_idx);
            return;
        }
        let owner = page.owner();
        if owner != PageOwner::Allocated && owner != PageOwner::Unknown {
            PCP_OWNER_MISMATCHES.fetch_add(1, Ordering::Relaxed);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[pcp] owner mismatch on free: cpu={} zone_idx={} owner={:?} page_ptr={:#x} -> buddy-fallback",
                    cpu,
                    zone_idx,
                    owner,
                    page as *mut Page as usize,
                );
            }
            fallback_to_buddy(page, zone_idx);
            return;
        }

        let pageset = cpu_pageset(cpu);
        let pcp = &pageset.pcp[zone_idx];

        // 释放到 PCP
        page.set_owner(PageOwner::Pcp);
        pcp.free(page);

        // 如果超过高水位，批量归还给 Buddy
        if pcp.above_high() {
            let mut zone = get_zone(idx_to_zone_type(zone_idx));
            if zone.initialized {
                let batch = pcp.batch;
                pcp.drain_to_buddy(&mut zone, batch);
            }
        }
    }
}

/// 排空所有 PCP (用于内存紧张时)
pub fn drain_all_pcps() {
    if !pcp_initialized() {
        return;
    }

    unsafe {
        for cpu in 0..nr_cpus() {
            let pageset = cpu_pageset(cpu);

            for zone_idx in 0..NR_PCP_LISTS {
                let pcp = &pageset.pcp[zone_idx];
                let mut zone = get_zone(idx_to_zone_type(zone_idx));

                if zone.initialized {
                    // 排空所有页
                    let batch = pcp.batch;
                    while pcp.nr_pages() > 0 {
                        pcp.drain_to_buddy(&mut zone, batch);
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
    /// 非法 free 请求计数
    pub invalid_free_rejects: u64,
    /// 可疑链表路径下的 Buddy 降级回收次数
    pub quarantine_fallbacks: u64,
    /// PCP 所有权不一致计数
    pub owner_mismatches: u64,
}

/// 获取 PCP 统计信息
pub fn pcp_stats() -> PcpStats {
    let mut stats = PcpStats {
        total_cached: 0,
        per_zone: [0; NR_PCP_LISTS],
        invalid_free_rejects: PCP_INVALID_FREE_REJECTS.load(Ordering::Relaxed),
        quarantine_fallbacks: PCP_QUARANTINE_FALLBACKS.load(Ordering::Relaxed),
        owner_mismatches: PCP_OWNER_MISMATCHES.load(Ordering::Relaxed),
    };

    if !pcp_initialized() {
        return stats;
    }

    unsafe {
        for cpu in 0..nr_cpus() {
            let pageset = cpu_pageset(cpu);

            for zone_idx in 0..NR_PCP_LISTS {
                let count = pageset.pcp[zone_idx].nr_pages() as u64;
                stats.per_zone[zone_idx] += count;
                stats.total_cached += count;
            }
        }
    }

    stats
}

#[inline]
fn cpu_pageset(cpu: usize) -> &'static PerCpuPageset {
    unsafe { &(*CPU_PAGESETS.inner.get())[cpu] }
}

#[inline]
unsafe fn cpu_pageset_mut(cpu: usize) -> &'static mut PerCpuPageset {
    unsafe { &mut (*CPU_PAGESETS.inner.get())[cpu] }
}
