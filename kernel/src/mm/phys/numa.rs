// ============================================================================
// january_os - NUMA (Non-Uniform Memory Access) 支持
//
// 多节点内存架构支持，为大型服务器优化内存访问
// ============================================================================

use super::page::ListHead;
use super::zone::{FreeArea, MAX_ORDER, NR_ZONES, Zone, ZoneType};
use crate::config;
use crate::interrupt::local_apic_id;
use crate::mm::vm::layout::PAGE_SIZE;
use crate::sync::Once;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// ============================================================================
// 常量 (从配置导入)
// ============================================================================

/// 最大 NUMA 节点数
pub const MAX_NUMNODES: usize = config::MAX_NUMA_NODES;

/// 无效节点 ID
pub const NUMA_NO_NODE: i32 = -1;

// ============================================================================
// NUMA 策略
// ============================================================================

/// 内存分配策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NumaPolicy {
    /// 默认：优先本地节点
    Default = 0,
    /// 仅本地节点
    Local = 1,
    /// 交错分配 (round-robin)
    Interleave = 2,
    /// 绑定到指定节点
    Bind = 3,
    /// 优先指定节点
    Preferred = 4,
}

impl Default for NumaPolicy {
    fn default() -> Self {
        Self::Default
    }
}

// ============================================================================
// 节点数据
// ============================================================================

/// NUMA 节点描述符 (类似 Linux pg_data_t)
#[repr(C)]
pub struct PgData {
    /// 节点 ID
    pub node_id: u32,
    /// 是否在线
    pub online: AtomicBool,
    /// 该节点的 Zones
    pub node_zones: [Zone; NR_ZONES],
    /// 节点起始 PFN
    pub node_start_pfn: u64,
    /// 节点总页数 (包含空洞)
    pub node_spanned_pages: u64,
    /// 节点实际存在的页数
    pub node_present_pages: u64,
    /// 节点名称
    pub name: &'static str,
    /// 下一个节点
    pub next: *mut PgData,

    // ========== 统计信息 ==========
    /// 空闲页数
    pub nr_free_pages: AtomicU32,
    /// 活跃页数
    pub nr_active_pages: AtomicU32,
    /// 不活跃页数
    pub nr_inactive_pages: AtomicU32,
}

impl PgData {
    pub const fn uninit() -> Self {
        Self {
            node_id: 0,
            online: AtomicBool::new(false),
            node_zones: [Zone::uninit(), Zone::uninit(), Zone::uninit()],
            node_start_pfn: 0,
            node_spanned_pages: 0,
            node_present_pages: 0,
            name: "",
            next: core::ptr::null_mut(),
            nr_free_pages: AtomicU32::new(0),
            nr_active_pages: AtomicU32::new(0),
            nr_inactive_pages: AtomicU32::new(0),
        }
    }

    /// 初始化节点
    pub fn init(&mut self, node_id: u32, start_pfn: u64, nr_pages: u64) {
        self.node_id = node_id;
        self.node_start_pfn = start_pfn;
        self.node_spanned_pages = nr_pages;
        self.node_present_pages = nr_pages;
        self.online.store(true, Ordering::SeqCst);

        // 初始化 Zones
        for zone in self.node_zones.iter_mut() {
            for area in zone.free_area.iter_mut() {
                area.init();
            }
        }
    }

    /// 获取节点总空闲页数
    pub fn total_free_pages(&self) -> u64 {
        use super::zone::get_zone;

        // UMA 模式：使用全局 Zones
        if self.node_id == 0 {
            let mut total = 0u64;
            for zone_type in [ZoneType::Dma, ZoneType::Dma32, ZoneType::Normal] {
                let zone = get_zone(zone_type);
                if zone.initialized {
                    total += zone.nr_free_pages();
                }
            }
            return total;
        }

        // NUMA 模式：使用节点内部 Zones
        let mut total = 0u64;
        for zone in self.node_zones.iter() {
            if zone.initialized {
                total += zone.nr_free_pages();
            }
        }
        total
    }

    /// 获取指定 Zone
    pub fn get_zone(&mut self, zone_type: ZoneType) -> &mut Zone {
        &mut self.node_zones[zone_type as usize]
    }

    /// 结束 PFN
    #[inline]
    pub fn node_end_pfn(&self) -> u64 {
        self.node_start_pfn + self.node_spanned_pages
    }

    /// 检查 PFN 是否属于此节点
    #[inline]
    pub fn contains_pfn(&self, pfn: u64) -> bool {
        pfn >= self.node_start_pfn && pfn < self.node_end_pfn()
    }
}

// ============================================================================
// 全局节点数据
// ============================================================================

/// 所有节点数据
struct NodeDataState {
    inner: UnsafeCell<[PgData; MAX_NUMNODES]>,
}

unsafe impl Sync for NodeDataState {}

impl NodeDataState {
    const fn new() -> Self {
        const UNINIT: PgData = PgData::uninit();
        Self {
            inner: UnsafeCell::new([UNINIT; MAX_NUMNODES]),
        }
    }
}

static NODE_DATA: NodeDataState = NodeDataState::new();

/// 在线节点数
static NR_ONLINE_NODES: AtomicU32 = AtomicU32::new(0);

/// NUMA 初始化标志
static NUMA_INIT: Once = Once::new();

/// 是否为 NUMA 系统 (false = UMA)
static IS_NUMA: AtomicBool = AtomicBool::new(false);

/// APIC ID 到 NUMA 节点的映射
/// 假设最大 APIC ID 为 255
struct ApicToNodeState {
    inner: UnsafeCell<[u32; 256]>,
}

unsafe impl Sync for ApicToNodeState {}

impl ApicToNodeState {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new([0; 256]),
        }
    }
}

static APIC_TO_NODE: ApicToNodeState = ApicToNodeState::new();

#[inline]
fn node_data_ref(node_id: usize) -> &'static PgData {
    unsafe { &(*NODE_DATA.inner.get())[node_id] }
}

#[inline]
unsafe fn node_data_mut(node_id: usize) -> &'static mut PgData {
    unsafe { &mut (*NODE_DATA.inner.get())[node_id] }
}

#[inline]
fn apic_to_node(apic_id: usize) -> u32 {
    unsafe { (*APIC_TO_NODE.inner.get())[apic_id] }
}

#[inline]
fn set_apic_to_node(apic_id: usize, node_id: u32) {
    unsafe {
        (*APIC_TO_NODE.inner.get())[apic_id] = node_id;
    }
}

// ============================================================================
// 节点访问
// ============================================================================

/// 获取节点数据
///
/// # Safety
///
/// node_id 必须有效
pub unsafe fn get_node_data(node_id: u32) -> &'static mut PgData {
    debug_assert!((node_id as usize) < MAX_NUMNODES);
    unsafe { node_data_mut(node_id as usize) }
}

/// 获取当前 CPU 的本地节点
#[inline]
pub fn numa_node_id() -> u32 {
    let apic_id = local_apic_id() as usize;
    if apic_id < 256 {
        apic_to_node(apic_id)
    } else {
        0
    }
}

/// 获取在线节点数
pub fn nr_online_nodes() -> u32 {
    NR_ONLINE_NODES.load(Ordering::Relaxed)
}

/// 检查是否为 NUMA 系统
pub fn is_numa() -> bool {
    IS_NUMA.load(Ordering::Relaxed)
}

/// 检查节点是否在线
pub fn node_online(node_id: u32) -> bool {
    if (node_id as usize) >= MAX_NUMNODES {
        return false;
    }
    node_data_ref(node_id as usize)
        .online
        .load(Ordering::Relaxed)
}

// ============================================================================
// 初始化
// ============================================================================

/// NUMA 节点信息 (用于初始化)
#[derive(Debug, Clone, Copy)]
pub struct NumaNodeInfo {
    /// 节点 ID
    pub node_id: u32,
    /// 起始物理地址
    pub start_addr: u64,
    /// 大小 (字节)
    pub size: u64,
    /// 关联的 CPU 掩码
    pub cpu_mask: u64,
}

/// 检查配置是否启用 NUMA
#[inline]
pub fn numa_config_enabled() -> bool {
    config::MEMORY_MODEL_NUMA
}

/// 检查配置是否为 UMA 模式
#[inline]
pub fn uma_config_enabled() -> bool {
    config::MEMORY_MODEL_UMA
}

/// 初始化 NUMA 子系统
///
/// 根据配置和硬件信息初始化：
/// - 如果配置为 UMA 模式，使用 UMA
/// - 如果配置启用但只有单节点，使用 UMA 模式
/// - 否则启用完整 NUMA 支持
///
/// # Arguments
/// * `nodes` - 节点信息数组 (从 ACPI SRAT 表解析)
pub unsafe fn init_numa(nodes: &[NumaNodeInfo]) {
    // 检查配置是否禁用 NUMA
    if config::MEMORY_MODEL_UMA {
        init_uma();
        return;
    }

    if nodes.is_empty() {
        // 无 NUMA 信息，使用 UMA 模式
        init_uma();
        return;
    }

    if nodes.len() == 1 {
        // 单节点，使用 UMA 模式
        init_uma();
        return;
    }

    // 多节点，NUMA 模式
    IS_NUMA.store(true, Ordering::SeqCst);

    for info in nodes.iter() {
        if (info.node_id as usize) >= MAX_NUMNODES {
            continue;
        }

        // 更新 CPU 到节点的映射
        let mut mask = info.cpu_mask;
        let mut apic_id = 0;
        while mask > 0 {
            if mask & 1 != 0 {
                if apic_id < 256 {
                    set_apic_to_node(apic_id, info.node_id);
                }
            }
            mask >>= 1;
            apic_id += 1;
        }

        let node = node_data_mut(info.node_id as usize);
        let start_pfn = info.start_addr / PAGE_SIZE;
        let nr_pages = info.size / PAGE_SIZE;

        node.init(info.node_id, start_pfn, nr_pages);
        NR_ONLINE_NODES.fetch_add(1, Ordering::Relaxed);
    }

    // 标记初始化完成（通过空闭包，因为实际初始化已在上面完成）
    NUMA_INIT.call_once(|| {});
}

/// 初始化 UMA 模式 (单节点)
///
/// 从现有 Zone 统计中获取内存信息
pub fn init_uma() {
    use super::zone::get_zone;

    IS_NUMA.store(false, Ordering::SeqCst);

    unsafe {
        let node = node_data_mut(0);
        node.node_id = 0;
        node.online.store(true, Ordering::SeqCst);

        // 从全局 Zones 计算节点内存统计
        let mut total_pages = 0u64;
        let mut start_pfn = u64::MAX;
        let mut end_pfn = 0u64;

        for zone_type in [ZoneType::Dma, ZoneType::Dma32, ZoneType::Normal] {
            let zone = get_zone(zone_type);
            if zone.initialized {
                total_pages += zone.present_pages;
                if zone.start_pfn < start_pfn {
                    start_pfn = zone.start_pfn;
                }
                let zone_end = zone.start_pfn + zone.spanned_pages;
                if zone_end > end_pfn {
                    end_pfn = zone_end;
                }
            }
        }

        node.node_start_pfn = if start_pfn == u64::MAX { 0 } else { start_pfn };
        node.node_spanned_pages = end_pfn.saturating_sub(node.node_start_pfn);
        node.node_present_pages = total_pages;
    }

    NR_ONLINE_NODES.store(1, Ordering::SeqCst);

    // 标记初始化完成
    NUMA_INIT.call_once(|| {});
}

/// 检查 NUMA 是否已初始化
pub fn numa_initialized() -> bool {
    NUMA_INIT.is_completed()
}

// ============================================================================
// 节点选择
// ============================================================================

/// 根据策略选择分配节点
pub fn select_node(policy: NumaPolicy, preferred: i32) -> u32 {
    match policy {
        NumaPolicy::Default | NumaPolicy::Local => {
            // 优先本地节点
            numa_node_id()
        }
        NumaPolicy::Bind | NumaPolicy::Preferred => {
            // 使用指定节点
            if preferred >= 0 && node_online(preferred as u32) {
                preferred as u32
            } else {
                numa_node_id()
            }
        }
        NumaPolicy::Interleave => {
            // 交错分配
            interleave_node()
        }
    }
}

/// 交错分配的下一个节点
static INTERLEAVE_NEXT: AtomicU32 = AtomicU32::new(0);

fn interleave_node() -> u32 {
    let nr_nodes = nr_online_nodes();
    if nr_nodes <= 1 {
        return 0;
    }

    // 简单 round-robin
    let next = INTERLEAVE_NEXT.fetch_add(1, Ordering::Relaxed);
    let mut node_id = next % nr_nodes;

    // 确保选中的节点在线
    let mut tries = 0;
    while !node_online(node_id) && tries < nr_nodes {
        node_id = (node_id + 1) % nr_nodes;
        tries += 1;
    }

    node_id
}

/// 获取备用节点列表 (当本地节点内存不足时)
pub fn get_fallback_nodes(node_id: u32) -> &'static [u32] {
    // 简化实现：返回所有其他节点
    // 真实实现应考虑 NUMA 距离
    static FALLBACK: [u32; MAX_NUMNODES] = [0, 1, 2, 3, 4, 5, 6, 7];

    let nr_nodes = nr_online_nodes() as usize;
    if nr_nodes <= 1 {
        return &[];
    }

    // 跳过本节点
    let start = (node_id as usize + 1) % nr_nodes;
    &FALLBACK[start..nr_nodes.min(MAX_NUMNODES)]
}

// ============================================================================
// NUMA 距离
// ============================================================================

/// NUMA 距离表 (节点间访问延迟)
struct NumaDistanceState {
    inner: UnsafeCell<[[u8; MAX_NUMNODES]; MAX_NUMNODES]>,
}

unsafe impl Sync for NumaDistanceState {}

impl NumaDistanceState {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new([[10u8; MAX_NUMNODES]; MAX_NUMNODES]),
        }
    }
}

static NUMA_DISTANCE: NumaDistanceState = NumaDistanceState::new();

#[inline]
fn numa_distance_ref(from: usize, to: usize) -> u8 {
    unsafe { (*NUMA_DISTANCE.inner.get())[from][to] }
}

#[inline]
unsafe fn numa_distance_mut(from: usize, to: usize, distance: u8) {
    unsafe {
        (*NUMA_DISTANCE.inner.get())[from][to] = distance;
    }
}

/// 本地访问距离
pub const LOCAL_DISTANCE: u8 = 10;

/// 远程访问距离 (默认)
pub const REMOTE_DISTANCE: u8 = 20;

/// 设置节点间距离
///
/// # Safety
///
/// 节点 ID 必须有效
pub unsafe fn set_numa_distance(from: u32, to: u32, distance: u8) {
    if (from as usize) < MAX_NUMNODES && (to as usize) < MAX_NUMNODES {
        unsafe { numa_distance_mut(from as usize, to as usize, distance) };
    }
}

/// 获取节点间距离
pub fn numa_distance(from: u32, to: u32) -> u8 {
    if (from as usize) >= MAX_NUMNODES || (to as usize) >= MAX_NUMNODES {
        return REMOTE_DISTANCE;
    }
    numa_distance_ref(from as usize, to as usize)
}

// ============================================================================
// 统计信息
// ============================================================================

/// NUMA 统计信息
pub struct NumaStats {
    /// 节点数
    pub nr_nodes: u32,
    /// 是否为 NUMA 系统
    pub is_numa: bool,
    /// 每个节点的信息
    pub nodes: [NodeStats; MAX_NUMNODES],
}

/// 单个节点的统计
#[derive(Clone, Copy, Default)]
pub struct NodeStats {
    /// 节点 ID
    pub node_id: u32,
    /// 是否在线
    pub online: bool,
    /// 总页数
    pub total_pages: u64,
    /// 空闲页数
    pub free_pages: u64,
}

/// 获取 NUMA 统计信息
pub fn numa_stats() -> NumaStats {
    let mut stats = NumaStats {
        nr_nodes: nr_online_nodes(),
        is_numa: is_numa(),
        nodes: [NodeStats::default(); MAX_NUMNODES],
    };

    unsafe {
        for i in 0..MAX_NUMNODES {
            let node = node_data_ref(i);
            stats.nodes[i] = NodeStats {
                node_id: node.node_id,
                online: node.online.load(Ordering::Relaxed),
                total_pages: node.node_present_pages,
                free_pages: node.total_free_pages(),
            };
        }
    }

    stats
}
