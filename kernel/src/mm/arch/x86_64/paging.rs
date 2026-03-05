// ============================================================================
// january_os - x86_64 页表管理模块
//
// 实现 x86_64 四级页表管理 (PML4 -> PDPT -> PD -> PT)
// ============================================================================

use crate::config;
use crate::interrupt;
use crate::mm::buddy::{alloc_pages, free_page};
use crate::mm::page::{max_pfn, page_to_pfn, pfn_to_page, PageFlags, PageOwner};
use crate::mm::zone::GFP_KERNEL_ZERO;
use crate::sync::{IrqSpinLock, SpinLock};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ============================================================================
// 页表条目标志位 (x86_64 特定)
// ============================================================================

/// 页表条目存在位
pub const PTE_PRESENT: u64 = 1 << 0;
/// 页表条目可写位
pub const PTE_WRITABLE: u64 = 1 << 1;
/// 用户态可访问位
pub const PTE_USER: u64 = 1 << 2;
/// 直写模式 (Write-Through)
pub const PTE_WRITE_THROUGH: u64 = 1 << 3;
/// 禁止缓存
pub const PTE_NO_CACHE: u64 = 1 << 4;
/// 已访问位
pub const PTE_ACCESSED: u64 = 1 << 5;
/// 已修改位 (脏位)
pub const PTE_DIRTY: u64 = 1 << 6;
/// 大页面标志 (2MB 或 1GB)
pub const PTE_HUGE: u64 = 1 << 7;
/// 全局页面 (不随 CR3 切换而刷新 TLB)
pub const PTE_GLOBAL: u64 = 1 << 8;
/// 禁止执行 (需要 NX 位支持)
pub const PTE_NO_EXECUTE: u64 = 1 << 63;

/// 地址掩码 (提取页帧地址)
pub const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

// ============================================================================
// 页表层级
// ============================================================================

/// 页表层级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableLevel {
    /// PML5 (5-level paging root)
    Pml5 = 5,
    /// PML4 (Page Map Level 4) - 512 GB 每项
    Pml4 = 4,
    /// PDPT (Page Directory Pointer Table) - 1 GB 每项
    Pdpt = 3,
    /// PD (Page Directory) - 2 MB 每项
    Pd = 2,
    /// PT (Page Table) - 4 KB 每项
    Pt = 1,
}

impl PageTableLevel {
    /// 获取每个条目覆盖的地址范围大小
    pub const fn entry_size(&self) -> u64 {
        match self {
            PageTableLevel::Pml5 => 256 * 1024 * 1024 * 1024 * 1024, // 256 TB
            PageTableLevel::Pml4 => 512 * 1024 * 1024 * 1024,        // 512 GB
            PageTableLevel::Pdpt => 1024 * 1024 * 1024,              // 1 GB
            PageTableLevel::Pd => 2 * 1024 * 1024,                   // 2 MB
            PageTableLevel::Pt => config::PAGE_SIZE,                 // 4 KB
        }
    }

    /// 获取下一级页表层级
    pub const fn next_level(&self) -> Option<PageTableLevel> {
        match self {
            PageTableLevel::Pml5 => Some(PageTableLevel::Pml4),
            PageTableLevel::Pml4 => Some(PageTableLevel::Pdpt),
            PageTableLevel::Pdpt => Some(PageTableLevel::Pd),
            PageTableLevel::Pd => Some(PageTableLevel::Pt),
            PageTableLevel::Pt => None,
        }
    }
}

// ============================================================================
// 页表条目
// ============================================================================

/// 页表条目
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// 创建空条目
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 创建新条目
    pub const fn new(phys_addr: u64, flags: u64) -> Self {
        Self((phys_addr & PTE_ADDR_MASK) | flags)
    }

    /// 获取原始值
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// 检查是否存在
    pub const fn is_present(&self) -> bool {
        self.0 & PTE_PRESENT != 0
    }

    /// 检查是否可写
    pub const fn is_writable(&self) -> bool {
        self.0 & PTE_WRITABLE != 0
    }

    /// 检查是否为用户态
    pub const fn is_user(&self) -> bool {
        self.0 & PTE_USER != 0
    }

    /// 检查是否为大页面
    pub const fn is_huge(&self) -> bool {
        self.0 & PTE_HUGE != 0
    }

    /// 检查是否禁止执行
    pub const fn is_no_execute(&self) -> bool {
        self.0 & PTE_NO_EXECUTE != 0
    }

    /// 获取物理地址
    pub const fn phys_addr(&self) -> u64 {
        self.0 & PTE_ADDR_MASK
    }

    /// 获取标志位
    pub const fn flags(&self) -> u64 {
        self.0 & !PTE_ADDR_MASK
    }

    /// 设置存在位
    pub fn set_present(&mut self, present: bool) {
        if present {
            self.0 |= PTE_PRESENT;
        } else {
            self.0 &= !PTE_PRESENT;
        }
    }

    /// 设置可写位
    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.0 |= PTE_WRITABLE;
        } else {
            self.0 &= !PTE_WRITABLE;
        }
    }

    /// 设置用户态可访问位
    pub fn set_user(&mut self, user: bool) {
        if user {
            self.0 |= PTE_USER;
        } else {
            self.0 &= !PTE_USER;
        }
    }

    /// 设置物理地址
    pub fn set_phys_addr(&mut self, addr: u64) {
        self.0 = (self.0 & !PTE_ADDR_MASK) | (addr & PTE_ADDR_MASK);
    }
}

impl core::fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PTE({:#018x} | ", self.phys_addr())?;
        if self.is_present() {
            write!(f, "P")?;
        } else {
            write!(f, "-")?;
        }
        if self.is_writable() {
            write!(f, "W")?;
        } else {
            write!(f, "R")?;
        }
        if self.is_user() {
            write!(f, "U")?;
        } else {
            write!(f, "S")?;
        }
        if self.is_huge() {
            write!(f, "H")?;
        } else {
            write!(f, "-")?;
        }
        if self.is_no_execute() {
            write!(f, "X")?;
        } else {
            write!(f, "-")?;
        }
        write!(f, ")")
    }
}

// ============================================================================
// 页表
// ============================================================================

/// 页表 (512 条目，占 4KB)
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    /// 创建空页表
    pub const fn empty() -> Self {
        const EMPTY: PageTableEntry = PageTableEntry::empty();
        Self {
            entries: [EMPTY; 512],
        }
    }

    /// 获取条目引用
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }

    /// 获取条目可变引用
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }

    /// 获取所有条目
    pub fn entries(&self) -> &[PageTableEntry; 512] {
        &self.entries
    }

    /// 获取所有条目可变引用
    pub fn entries_mut(&mut self) -> &mut [PageTableEntry; 512] {
        &mut self.entries
    }

    /// 清空页表
    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = PageTableEntry::empty();
        }
    }
}

// ============================================================================
// 地址索引计算 (x86_64 特定)
// ============================================================================

/// 从虚拟地址提取 PML4 索引
#[inline]
pub const fn pml4_index(virt: u64) -> usize {
    ((virt >> 39) & 0x1FF) as usize
}

/// 从虚拟地址提取 PML5 索引
#[inline]
pub const fn pml5_index(virt: u64) -> usize {
    ((virt >> 48) & 0x1FF) as usize
}

/// 从虚拟地址提取 PDPT 索引
#[inline]
pub const fn pdpt_index(virt: u64) -> usize {
    ((virt >> 30) & 0x1FF) as usize
}

/// 从虚拟地址提取 PD 索引
#[inline]
pub const fn pd_index(virt: u64) -> usize {
    ((virt >> 21) & 0x1FF) as usize
}

/// 从虚拟地址提取 PT 索引
#[inline]
pub const fn pt_index(virt: u64) -> usize {
    ((virt >> 12) & 0x1FF) as usize
}

/// 从虚拟地址提取页内偏移
#[inline]
pub const fn page_offset(virt: u64) -> usize {
    (virt & 0xFFF) as usize
}

#[inline]
const fn level_shift(level: u8) -> u8 {
    12 + (level - 1) * 9
}

/// 按页表层级提取索引：
/// - level=5 => PML5 index
/// - level=4 => PML4 index
/// - level=3 => PDPT index
/// - level=2 => PD index
/// - level=1 => PT index
#[inline]
pub const fn level_index(virt: u64, level: u8) -> usize {
    ((virt >> level_shift(level)) & 0x1FF) as usize
}

// ============================================================================
// 页表管理器
// ============================================================================

/// 页表管理器
///
/// 负责管理内核页表，提供虚拟地址映射功能
pub struct PageTableManager {
    /// 根页表物理地址（4-level 为 PML4，5-level 为 PML5）
    root_phys: u64,
    /// 直接映射偏移
    direct_map_offset: u64,
    /// 页表层级（4/5）
    page_levels: u8,
    /// 规范地址位宽（48/57）
    va_bits: u8,
}

/// 全局页表操作锁
///
/// 当前内核页表仍是共享地址空间，先使用全局串行化避免并发改表造成损坏。
static PAGE_TABLE_OP_LOCK: IrqSpinLock<()> = IrqSpinLock::with_name((), "PageTableOps");

/// TLB shootdown ACK 计数
static TLB_SHOOTDOWN_ACKED: AtomicU32 = AtomicU32::new(0);

/// TLB shootdown 是否可用（超时后自动熔断）
static TLB_SHOOTDOWN_ENABLED: AtomicBool = AtomicBool::new(true);

/// 参与 TLB shootdown 的 CPU（按 APIC ID 跟踪）
const TLB_TARGET_CAPACITY: usize = if crate::config::MAX_APIC_IDS > 0 {
    crate::config::MAX_APIC_IDS
} else {
    1
};

struct TlbShootdownTargets {
    apic_ids: [u32; TLB_TARGET_CAPACITY],
    count: usize,
}

impl TlbShootdownTargets {
    const fn new() -> Self {
        Self {
            apic_ids: [0; TLB_TARGET_CAPACITY],
            count: 0,
        }
    }

    fn register_apic_id(&mut self, apic_id: u32) -> bool {
        for idx in 0..self.count {
            if self.apic_ids[idx] == apic_id {
                return false;
            }
        }
        if self.count >= self.apic_ids.len() {
            return false;
        }
        self.apic_ids[self.count] = apic_id;
        self.count += 1;
        true
    }
}

static TLB_SHOOTDOWN_TARGETS: SpinLock<TlbShootdownTargets> =
    SpinLock::with_name(TlbShootdownTargets::new(), "TlbShootdownTargets");

/// 一次性诊断日志开关
static TLB_SHOOTDOWN_SKIP_NOT_READY_LOGGED: AtomicBool = AtomicBool::new(false);
static TLB_SHOOTDOWN_IPI_RECEIVED_LOGGED: AtomicBool = AtomicBool::new(false);
static TLB_SHOOTDOWN_FIRST_SENT_LOGGED: AtomicBool = AtomicBool::new(false);
static TLB_SHOOTDOWN_REGISTERED_LOGGED: AtomicBool = AtomicBool::new(false);
static TLB_SHOOTDOWN_TIMEOUT_LOGGED: AtomicBool = AtomicBool::new(false);

/// 等待远端 CPU shootdown 的最大自旋次数
const TLB_SHOOTDOWN_TIMEOUT_SPINS: usize = 2_000_000;
/// 等待探测 IPI 响应的最大自旋次数
const TLB_PROBE_TIMEOUT_SPINS: usize = 2_000_000;

/// 远核 TLB 可见性探测状态
static TLB_PROBE_ACTIVE: AtomicBool = AtomicBool::new(false);
static TLB_PROBE_ADDR: AtomicU64 = AtomicU64::new(0);
static TLB_PROBE_EXPECTED: AtomicU64 = AtomicU64::new(0);
static TLB_PROBE_HANDLED: AtomicU32 = AtomicU32::new(0);
static TLB_PROBE_MATCHED: AtomicU32 = AtomicU32::new(0);
static PT_RECLAIM_STOP_NON_PGTABLE: AtomicU64 = AtomicU64::new(0);
static PT_RECLAIM_STOP_SHARED: AtomicU64 = AtomicU64::new(0);
static PT_RECLAIM_OWNER_MISMATCH: AtomicU64 = AtomicU64::new(0);
static PT_RECLAIM_OWNER_HEALED: AtomicU64 = AtomicU64::new(0);
// 暂保留 [3GiB, 4GiB) 给 LAPIC/IOAPIC/PCI ECAM 等低地址 MMIO 访问路径；
// 等 MMIO 路径全面切到 direct-map/ioremap 后再移除此保留窗口。
const LOW_IDENTITY_WINDOW_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const LOW_IDENTITY_STEP_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct PtReclaimStats {
    pub stop_non_pgtable: u64,
    pub stop_shared: u64,
    pub owner_mismatch: u64,
    pub owner_healed: u64,
}

#[inline]
pub fn pt_reclaim_stats() -> PtReclaimStats {
    PtReclaimStats {
        stop_non_pgtable: PT_RECLAIM_STOP_NON_PGTABLE.load(Ordering::Relaxed),
        stop_shared: PT_RECLAIM_STOP_SHARED.load(Ordering::Relaxed),
        owner_mismatch: PT_RECLAIM_OWNER_MISMATCH.load(Ordering::Relaxed),
        owner_healed: PT_RECLAIM_OWNER_HEALED.load(Ordering::Relaxed),
    }
}

#[inline]
fn flush_tlb_local_only(virt_addr: u64) {
    unsafe {
        core::arch::asm!(
            "invlpg [{}]",
            in(reg) virt_addr,
            options(nostack, preserves_flags)
        );
    }
}

#[inline]
fn flush_tlb_all_local_only() {
    unsafe {
        core::arch::asm!(
            "mov {tmp}, cr3",
            "mov cr3, {tmp}",
            tmp = out(reg) _,
            options(nostack, preserves_flags)
        );
    }
}

/// 注册当前 CPU 为可参与 TLB shootdown 的目标。
///
/// 仅在 APIC 已初始化后生效；重复调用会自动去重。
pub fn register_tlb_shootdown_cpu() {
    if !interrupt::apic_initialized() {
        return;
    }

    let apic_id = interrupt::local_apic_id();
    let added = {
        let mut targets = TLB_SHOOTDOWN_TARGETS.lock();
        targets.register_apic_id(apic_id)
    };

    if added
        && TLB_SHOOTDOWN_REGISTERED_LOGGED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
    {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[tlb] register shootdown cpu apic_id={}",
                apic_id,
            );
        }
    }
}

fn collect_tlb_shootdown_targets(
    exclude_apic_id: u32,
    out: &mut [u32; TLB_TARGET_CAPACITY],
) -> usize {
    let targets = TLB_SHOOTDOWN_TARGETS.lock();
    let mut count = 0usize;
    for idx in 0..targets.count {
        let apic_id = targets.apic_ids[idx];
        if apic_id == exclude_apic_id {
            continue;
        }
        if count >= out.len() {
            break;
        }
        out[count] = apic_id;
        count += 1;
    }
    count
}

/// 返回当前已注册的 TLB shootdown CPU 数（包含本核）
pub fn tlb_shootdown_registered_cpu_count() -> usize {
    let targets = TLB_SHOOTDOWN_TARGETS.lock();
    targets.count
}

#[inline]
fn send_vector_to_targets(targets: &[u32], vector: u8) {
    for apic_id in targets.iter().copied() {
        interrupt::send_ipi(
            apic_id,
            vector,
            interrupt::ICR_DELIVERY_FIXED,
            interrupt::ICR_SHORTHAND_NONE,
            interrupt::ICR_LEVEL_ASSERT,
            interrupt::ICR_TRIGGER_EDGE,
        );
    }
}

fn shootdown_other_cpus() {
    if !TLB_SHOOTDOWN_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    if !interrupt::initialized() || !interrupt::apic_initialized() {
        if TLB_SHOOTDOWN_SKIP_NOT_READY_LOGGED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[tlb] skip shootdown: interrupt/APIC not ready (int_init={} apic_init={})",
                    interrupt::initialized(),
                    interrupt::apic_initialized(),
                );
            }
        }
        return;
    }

    let self_apic_id = interrupt::local_apic_id();
    let mut target_apic_ids = [0u32; TLB_TARGET_CAPACITY];
    let target_count = collect_tlb_shootdown_targets(self_apic_id, &mut target_apic_ids);
    if target_count == 0 {
        return;
    }

    let target_count = target_count.min(u32::MAX as usize) as u32;
    TLB_SHOOTDOWN_ACKED.store(0, Ordering::SeqCst);

    if TLB_SHOOTDOWN_FIRST_SENT_LOGGED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_ok()
    {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[tlb] send shootdown ipi vector={:#x} targets={} from_apic_id={}",
                interrupt::IPI_TLB_SHOOTDOWN,
                target_count,
                self_apic_id,
            );
        }
    }

    send_vector_to_targets(
        &target_apic_ids[..(target_count as usize)],
        interrupt::IPI_TLB_SHOOTDOWN,
    );

    let mut spins = 0usize;
    while TLB_SHOOTDOWN_ACKED.load(Ordering::Acquire) < target_count {
        core::hint::spin_loop();
        spins += 1;
        if spins >= TLB_SHOOTDOWN_TIMEOUT_SPINS {
            if TLB_SHOOTDOWN_TIMEOUT_LOGGED
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                crate::warn!(
                    "TLB shootdown timeout: acked={} target={} (keep enabled, retry on next flush)",
                    TLB_SHOOTDOWN_ACKED.load(Ordering::Relaxed),
                    target_count,
                );
            }
            break;
        }
    }
}

/// TLB shootdown IPI 处理（仅本地刷新）
pub fn handle_tlb_shootdown_ipi() {
    flush_tlb_all_local_only();
    if TLB_SHOOTDOWN_IPI_RECEIVED_LOGGED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_ok()
    {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[tlb] first shootdown IPI received on apic_id={}",
                interrupt::local_apic_id(),
            );
        }
    }
    TLB_SHOOTDOWN_ACKED.fetch_add(1, Ordering::AcqRel);
}

/// TLB probe IPI 处理（仅采样当前 CPU 对某个 VA 的可见值）
pub fn handle_tlb_probe_ipi() {
    if !TLB_PROBE_ACTIVE.load(Ordering::Acquire) {
        return;
    }

    let addr = TLB_PROBE_ADDR.load(Ordering::Acquire);
    let expect = TLB_PROBE_EXPECTED.load(Ordering::Acquire);
    if addr != 0 {
        // Avoid touching the probed VA directly on remote CPUs:
        // that VA may be unmapped in a private mm and would fault inside IPI context.
        let cr3 = crate::mm::arch::read_cr3() & PTE_ADDR_MASK;
        let pt_mgr = unsafe {
            PageTableManager::new_with_layout(
                cr3,
                crate::mm::direct_map_offset(),
                crate::mm::page_levels(),
                crate::mm::va_bits(),
            )
        };
        if let Some(phys) = pt_mgr.translate_addr(addr) {
            let value = unsafe { core::ptr::read_volatile(crate::mm::phys_to_virt(phys) as *const u64) };
            if value == expect {
                TLB_PROBE_MATCHED.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
    TLB_PROBE_HANDLED.fetch_add(1, Ordering::AcqRel);
}

/// 在其他 CPU 上探测某个 VA 的可见值。
///
/// 返回 `(targets, handled, matched)`：
/// - `targets`: 发送 IPI 的目标 CPU 数
/// - `handled`: 实际收到探测 IPI 并执行处理的 CPU 数
/// - `matched`: 读取到期望值的 CPU 数
pub fn run_tlb_probe_on_other_cpus(addr: u64, expected: u64) -> (u32, u32, u32) {
    if !interrupt::initialized() || !interrupt::apic_initialized() {
        return (0, 0, 0);
    }

    let self_apic_id = interrupt::local_apic_id();
    let mut target_apic_ids = [0u32; TLB_TARGET_CAPACITY];
    let target_count_usize = collect_tlb_shootdown_targets(self_apic_id, &mut target_apic_ids);
    if target_count_usize == 0 {
        return (0, 0, 0);
    }
    let target_count = target_count_usize.min(u32::MAX as usize) as u32;

    TLB_PROBE_ADDR.store(addr, Ordering::Release);
    TLB_PROBE_EXPECTED.store(expected, Ordering::Release);
    TLB_PROBE_HANDLED.store(0, Ordering::Release);
    TLB_PROBE_MATCHED.store(0, Ordering::Release);
    TLB_PROBE_ACTIVE.store(true, Ordering::Release);

    send_vector_to_targets(
        &target_apic_ids[..target_count as usize],
        interrupt::IPI_TLB_PROBE,
    );

    let mut spins = 0usize;
    while TLB_PROBE_HANDLED.load(Ordering::Acquire) < target_count {
        core::hint::spin_loop();
        spins += 1;
        if spins >= TLB_PROBE_TIMEOUT_SPINS {
            break;
        }
    }

    TLB_PROBE_ACTIVE.store(false, Ordering::Release);
    let handled = TLB_PROBE_HANDLED.load(Ordering::Acquire);
    let matched = TLB_PROBE_MATCHED.load(Ordering::Acquire);
    (target_count, handled, matched)
}

/// 清理启动期低地址 identity-map（默认窗口 0..3GiB）。
///
/// 返回成功移除的 1GiB 条目数量。
pub fn teardown_bootstrap_identity_map(direct_map_offset: u64) -> usize {
    let cr3_phys = crate::mm::arch::read_cr3() & PTE_ADDR_MASK;
    let pt_mgr = unsafe {
        PageTableManager::new_with_layout(
            cr3_phys,
            direct_map_offset,
            crate::mm::page_levels(),
            crate::mm::va_bits(),
        )
    };

    let mut removed = 0usize;
    let mut addr = 0u64;
    while addr < LOW_IDENTITY_WINDOW_BYTES {
        if unsafe { pt_mgr.unmap_page(addr) } {
            removed += 1;
        }
        addr = addr.saturating_add(LOW_IDENTITY_STEP_BYTES);
    }
    removed
}

#[inline]
fn entry_references_lower_table(entry: PageTableEntry) -> bool {
    entry.is_present() && !entry.is_huge()
}

#[inline]
unsafe fn retain_table_page_ref(table_phys: u64) {
    let pfn = table_phys / config::PAGE_SIZE;
    if pfn >= max_pfn() {
        return;
    }
    let page = unsafe { &*pfn_to_page(pfn) };
    page.get();
    page.set_flag(PageFlags::PGTABLE);
    page.set_owner(crate::mm::page::page::PageOwner::Pgtable);
}

#[inline]
unsafe fn release_table_page_ref(table_phys: u64) {
    let pfn = table_phys / config::PAGE_SIZE;
    if pfn >= max_pfn() {
        return;
    }
    let page = unsafe { &mut *pfn_to_page(pfn) };
    if !page.is_pgtable() && !page.is_reserved() {
        // 兼容早期路径：页表页可能缺失 PGTABLE/owner 元数据，允许在回收点按表项语义自愈。
        // 若 refcount 为 0 则视为真正异常，保持 mismatch 统计并拒绝释放。
        if page.refcount() == 0 {
            PT_RECLAIM_OWNER_MISMATCH.fetch_add(1, Ordering::Relaxed);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[pt] release table reject non-pgtable phys={:#x} pfn={} owner={:?} flags={:#x} refcount={}",
                    table_phys,
                    pfn,
                    page.owner(),
                    page.flags().bits(),
                    page.refcount()
                );
            }
            return;
        }
        page.set_flag(PageFlags::PGTABLE);
        page.set_owner(PageOwner::Pgtable);
        PT_RECLAIM_OWNER_HEALED.fetch_add(1, Ordering::Relaxed);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pt] release table healed metadata phys={:#x} pfn={} refcount={}",
                table_phys,
                pfn,
                page.refcount()
            );
        }
    }
    if page.is_pgtable() {
        let owner = page.owner();
        if owner != PageOwner::Pgtable && owner != PageOwner::Reserved {
            if page.refcount() == 0 {
                PT_RECLAIM_OWNER_MISMATCH.fetch_add(1, Ordering::Relaxed);
            } else {
                page.set_owner(PageOwner::Pgtable);
                PT_RECLAIM_OWNER_HEALED.fetch_add(1, Ordering::Relaxed);
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[pt] release table healed owner phys={:#x} pfn={} old_owner={:?} refcount={}",
                        table_phys,
                        pfn,
                        owner,
                        page.refcount()
                    );
                }
            }
        }
    }
    if page.refcount() == 0 {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pt] release table skipped (ref=0) phys={:#x} pfn={}",
                table_phys,
                pfn
            );
        }
        return;
    }
    if page.is_reserved() {
        let _ = page.try_put();
        return;
    }
    unsafe {
        free_page(page);
    }
}

/// 复制内核高半区根条目，并维护下级页表页引用计数。
///
/// # Safety
/// `src_root_phys` 和 `dst_root_phys` 必须是有效根页表物理地址。
pub unsafe fn clone_kernel_root_entries_with_refs(src_root_phys: u64, dst_root_phys: u64) {
    let src_root = src_root_phys & PTE_ADDR_MASK;
    let dst_root = dst_root_phys & PTE_ADDR_MASK;
    if src_root == 0 || dst_root == 0 {
        return;
    }
    let page_levels = crate::mm::page_levels();
    let va_bits = crate::mm::va_bits();
    let kernel_root_start = level_index(crate::mm::KERNEL_BASE, page_levels);
    let direct_map = crate::mm::direct_map_offset();

    unsafe {
        let _pt_guard = PAGE_TABLE_OP_LOCK.lock();
        let src_mgr = PageTableManager::new_with_layout(src_root, direct_map, page_levels, va_bits);
        let mut dst_mgr =
            PageTableManager::new_with_layout(dst_root, direct_map, page_levels, va_bits);
        let src_tbl = src_mgr.root_table();
        let dst_tbl = dst_mgr.root_table_mut();

        for idx in kernel_root_start..512 {
            let old_entry = *dst_tbl.entry(idx);
            let new_entry = *src_tbl.entry(idx);
            if old_entry.raw() == new_entry.raw() {
                continue;
            }
            if entry_references_lower_table(old_entry) {
                release_table_page_ref(old_entry.phys_addr());
            }
            if entry_references_lower_table(new_entry) {
                retain_table_page_ref(new_entry.phys_addr());
            }
            *dst_tbl.entry_mut(idx) = new_entry;
        }
    }
}

/// 释放一个私有地址空间根页表里“共享内核根条目”持有的下级页表引用。
///
/// # Safety
/// `root_phys` 必须是有效根页表物理地址，调用方需保证该地址空间不再并发使用。
pub unsafe fn release_kernel_root_entries_refs(root_phys: u64) {
    let root = root_phys & PTE_ADDR_MASK;
    if root == 0 {
        return;
    }
    let page_levels = crate::mm::page_levels();
    let va_bits = crate::mm::va_bits();
    let kernel_root_start = level_index(crate::mm::KERNEL_BASE, page_levels);
    let direct_map = crate::mm::direct_map_offset();

    unsafe {
        let _pt_guard = PAGE_TABLE_OP_LOCK.lock();
        let mut mgr = PageTableManager::new_with_layout(root, direct_map, page_levels, va_bits);
        let root_tbl = mgr.root_table_mut();
        for idx in kernel_root_start..512 {
            let entry = *root_tbl.entry(idx);
            if !entry_references_lower_table(entry) {
                continue;
            }
            let child_phys = entry.phys_addr();
            let pfn = child_phys / config::PAGE_SIZE;
            if pfn >= max_pfn() {
                continue;
            }
            let child_page = &*pfn_to_page(pfn);
            if !child_page.is_pgtable() {
                continue;
            }
            release_table_page_ref(child_phys);
        }
    }
}

/// 将目标根页表的内核高半区根条目同步为 init_mm 的当前视图。
///
/// 返回是否发生了条目变更。
pub fn sync_kernel_root_entries_from_init(dst_root_phys: u64) -> bool {
    let dst_root = dst_root_phys & PTE_ADDR_MASK;
    let init_root = unsafe { (*crate::mm::init_mm_ptr()).pgd } & PTE_ADDR_MASK;
    if dst_root == 0 || init_root == 0 || dst_root == init_root {
        return false;
    }

    let page_levels = crate::mm::page_levels();
    let va_bits = crate::mm::va_bits();
    let kernel_root_start = level_index(crate::mm::KERNEL_BASE, page_levels);
    let direct_map = crate::mm::direct_map_offset();

    unsafe {
        let _pt_guard = PAGE_TABLE_OP_LOCK.lock();
        let src_mgr =
            PageTableManager::new_with_layout(init_root, direct_map, page_levels, va_bits);
        let mut dst_mgr =
            PageTableManager::new_with_layout(dst_root, direct_map, page_levels, va_bits);
        let src_root = src_mgr.root_table();
        let dst_root_tbl = dst_mgr.root_table_mut();
        let mut changed = false;

        for idx in kernel_root_start..512 {
            let src_entry = *src_root.entry(idx);
            let dst_entry = dst_root_tbl.entry_mut(idx);
            if dst_entry.raw() != src_entry.raw() {
                if entry_references_lower_table(*dst_entry) {
                    release_table_page_ref(dst_entry.phys_addr());
                }
                if entry_references_lower_table(src_entry) {
                    retain_table_page_ref(src_entry.phys_addr());
                }
                *dst_entry = src_entry;
                changed = true;
            }
        }

        if changed {
            // 若当前 CPU 正在使用该 root，则本地立即刷新；其他 CPU 通过 IPI 刷新。
            let current_root = crate::mm::arch::read_cr3() & PTE_ADDR_MASK;
            if current_root == dst_root {
                flush_tlb_all_local_only();
            }
            shootdown_other_cpus();
        }

        changed
    }
}

impl PageTableManager {
    /// 创建页表管理器
    ///
    /// # Safety
    /// `root_phys` 必须指向有效根页表（4-level 为 PML4，5-level 为 PML5）
    pub unsafe fn new(root_phys: u64, direct_map_offset: u64) -> Self {
        let page_levels = crate::mm::vm::layout_runtime::page_levels();
        let va_bits = crate::mm::vm::layout_runtime::va_bits();
        unsafe { Self::new_with_layout(root_phys, direct_map_offset, page_levels, va_bits) }
    }

    /// 创建页表管理器（显式指定页表层级与 VA 位宽）
    ///
    /// # Safety
    /// `root_phys` 必须指向与 `page_levels` 匹配的根页表。
    pub const unsafe fn new_with_layout(
        root_phys: u64,
        direct_map_offset: u64,
        page_levels: u8,
        va_bits: u8,
    ) -> Self {
        let levels = if page_levels == 5 { 5 } else { 4 };
        let bits = if levels == 5 && va_bits == 57 { 57 } else { 48 };
        Self {
            root_phys,
            direct_map_offset,
            page_levels: levels,
            va_bits: bits,
        }
    }

    /// 物理地址转虚拟地址（通过直接映射）
    #[inline]
    pub const fn phys_to_virt(&self, phys: u64) -> u64 {
        self.direct_map_offset + phys
    }

    /// 虚拟地址转物理地址（通过直接映射，仅对直接映射区有效）
    #[inline]
    pub const fn virt_to_phys(&self, virt: u64) -> u64 {
        virt - self.direct_map_offset
    }

    #[inline]
    pub const fn page_levels(&self) -> u8 {
        self.page_levels
    }

    #[inline]
    pub const fn va_bits(&self) -> u8 {
        self.va_bits
    }

    /// 获取根页表
    pub fn root_table(&self) -> &PageTable {
        unsafe { &*(self.phys_to_virt(self.root_phys) as *const PageTable) }
    }

    /// 获取根页表可变引用
    pub fn root_table_mut(&mut self) -> &mut PageTable {
        unsafe { &mut *(self.phys_to_virt(self.root_phys) as *mut PageTable) }
    }

    /// 获取根页表物理地址
    pub const fn root_phys(&self) -> u64 {
        self.root_phys
    }

    /// 兼容接口：返回 CR3 根页表地址。
    pub const fn pml4_phys(&self) -> u64 {
        self.root_phys
    }

    #[inline]
    unsafe fn table_ref(&self, phys: u64) -> &PageTable {
        unsafe { &*(self.phys_to_virt(phys) as *const PageTable) }
    }

    #[inline]
    unsafe fn table_mut(&self, phys: u64) -> &mut PageTable {
        unsafe { &mut *(self.phys_to_virt(phys) as *mut PageTable) }
    }

    #[inline]
    unsafe fn alloc_zeroed_table_phys(&self) -> Option<u64> {
        let page = alloc_pages(0, GFP_KERNEL_ZERO)?;
        let pfn = page_to_pfn(page);
        let table_phys = pfn * config::PAGE_SIZE;
        page.set_flag(PageFlags::PGTABLE);
        page.set_owner(crate::mm::page::page::PageOwner::Pgtable);
        // 防御式清零：避免分配器快路径遗漏 GFP_ZERO 语义时引入脏页表。
        unsafe {
            core::ptr::write_bytes(
                self.phys_to_virt(table_phys) as *mut u8,
                0,
                config::PAGE_SIZE as usize,
            );
        }
        Some(table_phys)
    }

    #[inline]
    unsafe fn split_huge_entry_for_user(&self, entry: &mut PageTableEntry, level: u8) -> bool {
        if !(level == 3 || level == 2) || !entry.is_present() || !entry.is_huge() {
            return false;
        }

        let base_phys = entry.phys_addr();
        let old_flags = entry.flags();
        let Some(child_phys) = (unsafe { self.alloc_zeroed_table_phys() }) else {
            return false;
        };
        let child = unsafe { self.table_mut(child_phys) };

        if level == 3 {
            // Split 1GiB mapping into 512x2MiB entries.
            let leaf_flags = old_flags;
            const SIZE_2M: u64 = 2 * 1024 * 1024;
            for i in 0..512usize {
                let phys = base_phys.saturating_add((i as u64) * SIZE_2M);
                *child.entry_mut(i) = PageTableEntry::new(phys, leaf_flags);
            }
        } else {
            // Split 2MiB mapping into 512x4KiB entries.
            let leaf_flags = old_flags & !PTE_HUGE;
            for i in 0..512usize {
                let phys = base_phys.saturating_add((i as u64) * config::PAGE_SIZE);
                *child.entry_mut(i) = PageTableEntry::new(phys, leaf_flags);
            }
        }

        let mut parent_flags = PTE_PRESENT;
        if (old_flags & PTE_WRITABLE) != 0 {
            parent_flags |= PTE_WRITABLE;
        }
        if (old_flags & PTE_USER) != 0 {
            parent_flags |= PTE_USER;
        }
        if (old_flags & PTE_WRITE_THROUGH) != 0 {
            parent_flags |= PTE_WRITE_THROUGH;
        }
        if (old_flags & PTE_NO_CACHE) != 0 {
            parent_flags |= PTE_NO_CACHE;
        }
        *entry = PageTableEntry::new(child_phys, parent_flags);
        true
    }

    /// 遍历页表，查找虚拟地址对应的页表条目
    ///
    /// 返回 (条目, 页表层级, 页面大小)
    pub fn translate(&self, virt_addr: u64) -> Option<(PageTableEntry, PageTableLevel, u64)> {
        let _pt_guard = PAGE_TABLE_OP_LOCK.lock();
        let mut table_phys = self.root_phys;

        for level in (2..=self.page_levels).rev() {
            let idx = level_index(virt_addr, level);
            let table = unsafe { self.table_ref(table_phys) };
            let entry = table.entry(idx);
            if !entry.is_present() {
                return None;
            }

            if level == 3 && entry.is_huge() {
                return Some((*entry, PageTableLevel::Pdpt, 1024 * 1024 * 1024));
            }
            if level == 2 && entry.is_huge() {
                return Some((*entry, PageTableLevel::Pd, 2 * 1024 * 1024));
            }

            table_phys = entry.phys_addr();
        }

        let pt = unsafe { self.table_ref(table_phys) };
        let pte = pt.entry(level_index(virt_addr, 1));
        if !pte.is_present() {
            return None;
        }
        Some((*pte, PageTableLevel::Pt, config::PAGE_SIZE))
    }

    /// 将虚拟地址转换为物理地址
    pub fn translate_addr(&self, virt_addr: u64) -> Option<u64> {
        let (entry, _level, page_size) = self.translate(virt_addr)?;
        let offset_mask = page_size - 1;
        Some(entry.phys_addr() + (virt_addr & offset_mask))
    }

    /// 刷新单个 TLB 条目
    pub fn flush_tlb(&self, virt_addr: u64) {
        self.flush_tlb_local(virt_addr);
        shootdown_other_cpus();
    }

    /// 刷新整个 TLB (重新加载 CR3)
    pub fn flush_tlb_all(&self) {
        self.flush_tlb_all_local();
        shootdown_other_cpus();
    }

    #[inline]
    fn flush_tlb_local(&self, virt_addr: u64) {
        flush_tlb_local_only(virt_addr);
    }

    #[inline]
    fn flush_tlb_all_local(&self) {
        flush_tlb_all_local_only();
    }

    #[inline]
    fn flush_tlb_global(&self, virt_addr: u64) {
        self.flush_tlb_local(virt_addr);
        shootdown_other_cpus();
    }

    /// 释放被重映射替换掉的旧页（仅针对参与 mapcount 的托管页）
    unsafe fn release_replaced_page(&self, old_phys: u64) {
        let pfn = old_phys / config::PAGE_SIZE;

        if pfn >= max_pfn() {
            return;
        }

        let old_page = &mut *pfn_to_page(pfn);

        // mapcount < 0 通常表示未纳入用户映射计数（例如内核 vmalloc 路径）
        if old_page.mapcount() < 0 {
            return;
        }

        let _ = old_page.try_dec_mapcount();

        if old_page.refcount() == 0 {
            return;
        }

        free_page(old_page);
    }

    /// 映射虚拟地址到物理地址
    ///
    /// # Safety
    ///
    /// - 需要确保分配内存成功
    pub unsafe fn map_page(&self, virt: u64, phys: u64, flags: u64) -> bool {
        let watch_vmalloc =
            crate::config::DEBUG_VERBOSE && crate::mm::vmalloc::is_vmalloc_watch_page(virt);
        if watch_vmalloc {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pt] map_page watch virt={:#x} phys={:#x} root={:#x} flags={:#x}",
                virt & !(config::PAGE_SIZE - 1),
                phys & PTE_ADDR_MASK,
                self.root_phys,
                flags
            );
        }
        let _pt_guard = PAGE_TABLE_OP_LOCK.lock();
        let mut table_phys = self.root_phys;
        let need_user = (flags & PTE_USER) != 0;
        let need_writable = (flags & PTE_WRITABLE) != 0;

        for level in (2..=self.page_levels).rev() {
            let idx = level_index(virt, level);
            let table = unsafe { self.table_mut(table_phys) };
            let entry = table.entry_mut(idx);

            if entry.is_huge() {
                // User mapping may overlap low huge mappings (runtime identity map).
                // Split huge entries on demand to continue descending.
                if need_user && (level == 3 || level == 2) {
                    if !(unsafe { self.split_huge_entry_for_user(entry, level) }) {
                        return false;
                    }
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[pt] split huge entry level={} virt={:#x} root={:#x}",
                            level,
                            virt & !(config::PAGE_SIZE - 1),
                            self.root_phys
                        );
                    }
                } else {
                    // 4KB 映射不能直接穿透已有 huge 映射。
                    return false;
                }
            }

            if !entry.is_present() {
                let Some(next_phys) = (unsafe { self.alloc_zeroed_table_phys() }) else {
                    return false;
                };
                entry.set_phys_addr(next_phys);
                entry.set_present(true);
                entry.set_writable(true);
                entry.set_user(true);
            } else {
                // User mapping may collide with a non-user shared subtree
                // (e.g. low runtime mapping borrowed from init_mm).
                // Clone the next-level table before setting U bit to avoid mutating shared roots.
                if need_user && !entry.is_user() {
                    let old_phys = entry.phys_addr();
                    let Some(new_phys) = (unsafe { self.alloc_zeroed_table_phys() }) else {
                        return false;
                    };
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            self.phys_to_virt(old_phys) as *const u8,
                            self.phys_to_virt(new_phys) as *mut u8,
                            config::PAGE_SIZE as usize,
                        );
                    }
                    entry.set_phys_addr(new_phys);
                    entry.set_present(true);
                    entry.set_user(true);
                    entry.set_writable(true);
                    if crate::config::DEBUG_VERBOSE {
                        crate::kprintln!(
                            "\x1b[90m[diag]\x1b[0m[pt] user-map COW upper table level={} virt={:#x} old={:#x} new={:#x} root={:#x}",
                            level,
                            virt & !(config::PAGE_SIZE - 1),
                            old_phys,
                            new_phys,
                            self.root_phys
                        );
                    }
                }
                if need_writable && !entry.is_writable() {
                    entry.set_writable(true);
                }
            }

            table_phys = entry.phys_addr();
        }

        let pt = unsafe { self.table_mut(table_phys) };
        let pt_idx = level_index(virt, 1);
        let pt_entry = pt.entry_mut(pt_idx);

        let was_present = pt_entry.is_present();

        // 如果已映射：处理重映射语义
        if was_present {
            let old_phys = pt_entry.phys_addr();
            let new_phys = phys & PTE_ADDR_MASK;

            // 仅在目标物理页变化时释放旧映射页
            if old_phys != new_phys {
                self.release_replaced_page(old_phys);
            }
        }

        *pt_entry = PageTableEntry::new(phys, flags);
        if watch_vmalloc {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pt] map_page watch committed virt={:#x} phys={:#x} root={:#x}",
                virt & !(config::PAGE_SIZE - 1),
                (phys & PTE_ADDR_MASK),
                self.root_phys
            );
        }

        // 释放页表大锁后再执行 TLB shootdown，避免锁持有期间等待远核 ACK。
        drop(_pt_guard);
        if was_present {
            self.flush_tlb_global(virt);
        } else {
            self.flush_tlb_local(virt);
        }

        true
    }

    /// 取消映射虚拟地址
    ///
    /// 成功返回 true，如果未映射返回 false。
    /// 空的中间页表页会被自动回收。
    pub unsafe fn unmap_page(&self, virt: u64) -> bool {
        let watch_vmalloc =
            crate::config::DEBUG_VERBOSE && crate::mm::vmalloc::is_vmalloc_watch_page(virt);
        if watch_vmalloc {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pt] unmap_page watch virt={:#x} root={:#x}",
                virt & !(config::PAGE_SIZE - 1),
                self.root_phys
            );
        }
        let _pt_guard = PAGE_TABLE_OP_LOCK.lock();
        const MAX_LEVELS: usize = 5;
        let mut parent_phys_path = [0u64; MAX_LEVELS];
        let mut parent_idx_path = [0usize; MAX_LEVELS];
        let mut path_len = 0usize;

        let mut table_phys = self.root_phys;
        for level in (2..=self.page_levels).rev() {
            let idx = level_index(virt, level);
            parent_phys_path[path_len] = table_phys;
            parent_idx_path[path_len] = idx;
            path_len += 1;

            let table = unsafe { self.table_mut(table_phys) };
            let entry = table.entry_mut(idx);
            if !entry.is_present() {
                return false;
            }

            if (level == 3 || level == 2) && entry.is_huge() {
                *entry = PageTableEntry::empty();
                drop(_pt_guard);
                self.flush_tlb_global(virt);
                return true;
            }

            table_phys = entry.phys_addr();
        }

        let pt_phys = table_phys;
        let pt = unsafe { self.table_mut(pt_phys) };
        let pt_idx = level_index(virt, 1);
        let pt_entry = pt.entry_mut(pt_idx);
        if !pt_entry.is_present() {
            return false;
        }
        *pt_entry = PageTableEntry::empty();
        if watch_vmalloc {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[pt] unmap_page watch cleared pte virt={:#x} root={:#x}",
                virt & !(config::PAGE_SIZE - 1),
                self.root_phys
            );
        }

        // 自底向上回收空页表（不回收根页表）。
        // 仅当页表页是“独占所有权(refcount==1)”时才清父项并回收。
        let mut child_phys = pt_phys;
        for i in (0..path_len).rev() {
            let child_table = unsafe { self.table_ref(child_phys) };
            if !Self::is_table_empty(child_table) {
                break;
            }

            let child_pfn = child_phys / config::PAGE_SIZE;
            if child_pfn >= max_pfn() {
                break;
            }
            let child_page = unsafe { &*pfn_to_page(child_pfn) };
            if !child_page.is_pgtable() {
                PT_RECLAIM_STOP_NON_PGTABLE.fetch_add(1, Ordering::Relaxed);
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[pt] reclaim stop(non-pgtable) child={:#x} pfn={} refcount={}",
                        child_phys,
                        child_pfn,
                        child_page.refcount()
                    );
                }
                break;
            }
            if child_page.refcount() != 1 {
                PT_RECLAIM_STOP_SHARED.fetch_add(1, Ordering::Relaxed);
                if watch_vmalloc {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[pt] reclaim stop(shared) child={:#x} refcount={}",
                        child_phys,
                        child_page.refcount()
                    );
                }
                break;
            }

            let parent_phys = parent_phys_path[i];
            let parent_idx = parent_idx_path[i];
            let parent = unsafe { self.table_mut(parent_phys) };
            *parent.entry_mut(parent_idx) = PageTableEntry::empty();
            Self::free_table_page(child_phys);
            child_phys = parent_phys;
        }

        drop(_pt_guard);
        self.flush_tlb_global(virt);
        true
    }

    /// 检查页表是否全空（所有条目都不存在）
    fn is_table_empty(table: &PageTable) -> bool {
        table.entries().iter().all(|e| !e.is_present())
    }

    fn free_table_page(table_phys: u64) {
        let pfn = table_phys / config::PAGE_SIZE;
        if pfn >= max_pfn() {
            return;
        }
        unsafe {
            let page = &mut *pfn_to_page(pfn);
            if crate::config::DEBUG_VERBOSE && !page.is_pgtable() {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[pt] free_table_page on non-pgtable page phys={:#x} pfn={} flags={:#x} refcount={}",
                    table_phys,
                    pfn,
                    page.flags().bits(),
                    page.refcount()
                );
            }
            release_table_page_ref(table_phys);
        }
    }

    /// 获取 PML4 中存在的条目数量
    pub fn count_pml4_entries(&self) -> usize {
        self.root_table()
            .entries()
            .iter()
            .filter(|e| e.is_present())
            .count()
    }
}
