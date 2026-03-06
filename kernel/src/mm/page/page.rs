// ============================================================================
// january_os - 页帧描述符 (struct page)
//
// 参考 Linux 内核设计，每个物理页帧都有一个对应的 Page 结构
// ============================================================================

use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageOwner {
    Unknown = 0,
    Buddy = 1,
    Pcp = 2,
    Slab = 3,
    Pgtable = 4,
    Allocated = 5,
    Reserved = 6,
}

impl PageOwner {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Buddy,
            2 => Self::Pcp,
            3 => Self::Slab,
            4 => Self::Pgtable,
            5 => Self::Allocated,
            6 => Self::Reserved,
            _ => Self::Unknown,
        }
    }
}

static PAGE_REF_UNDERFLOW_REJECTS: AtomicU64 = AtomicU64::new(0);
static PAGE_MAPCOUNT_UNDERFLOW_REJECTS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub struct PageGuardStats {
    pub ref_underflow_rejects: u64,
    pub mapcount_underflow_rejects: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageCounterError {
    RefUnderflow,
    MapcountUnderflow,
}

#[inline]
pub fn page_guard_stats() -> PageGuardStats {
    PageGuardStats {
        ref_underflow_rejects: PAGE_REF_UNDERFLOW_REJECTS.load(Ordering::Relaxed),
        mapcount_underflow_rejects: PAGE_MAPCOUNT_UNDERFLOW_REJECTS.load(Ordering::Relaxed),
    }
}

// ============================================================================
// 页帧标志位
// ============================================================================

/// 页帧标志位
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct PageFlags(u32);

impl PageFlags {
    /// 保留页（不可分配）
    pub const RESERVED: u32 = 1 << 0;
    /// 被 SLUB 使用
    pub const SLAB: u32 = 1 << 1;
    /// 在 Buddy 系统空闲链表中
    pub const BUDDY: u32 = 1 << 2;
    /// 复合页（大页的一部分）
    pub const COMPOUND: u32 = 1 << 3;
    /// 复合页头
    pub const HEAD: u32 = 1 << 4;
    /// 复合页尾
    pub const TAIL: u32 = 1 << 5;
    /// 脏页（已修改）
    pub const DIRTY: u32 = 1 << 6;
    /// 在 LRU 链表中
    pub const LRU: u32 = 1 << 7;
    /// 活跃页
    pub const ACTIVE: u32 = 1 << 8;
    /// 被锁定
    pub const LOCKED: u32 = 1 << 9;
    /// 正在回写
    pub const WRITEBACK: u32 = 1 << 10;
    /// 私有页
    pub const PRIVATE: u32 = 1 << 11;
    /// 页内容已更新
    pub const UPTODATE: u32 = 1 << 12;
    /// 匿名页
    pub const ANON: u32 = 1 << 13;
    /// 文件映射页
    pub const FILEMAPPED: u32 = 1 << 14;
    /// 交换页
    pub const SWAPCACHE: u32 = 1 << 15;
    /// 页表页
    pub const PGTABLE: u32 = 1 << 16;

    /// 创建空标志
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 创建带初始值的标志
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    /// 获取原始值
    pub const fn bits(&self) -> u32 {
        self.0
    }

    /// 设置标志位
    pub fn set(&mut self, flag: u32) {
        self.0 |= flag;
    }

    /// 清除标志位
    pub fn clear(&mut self, flag: u32) {
        self.0 &= !flag;
    }

    /// 测试标志位
    pub const fn test(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

// ============================================================================
// 链表节点
// ============================================================================

/// 双向链表头
#[repr(C)]
pub struct ListHead {
    pub next: *mut ListHead,
    pub prev: *mut ListHead,
}

impl ListHead {
    /// 创建空链表头
    pub const fn new() -> Self {
        Self {
            next: core::ptr::null_mut(),
            prev: core::ptr::null_mut(),
        }
    }

    /// 初始化为指向自己（空链表）
    pub fn init(&mut self) {
        self.next = self as *mut _;
        self.prev = self as *mut _;
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.next == self as *const _ as *mut _
    }

    /// 在 head 之后插入 new
    pub unsafe fn add(&mut self, new: *mut ListHead) {
        unsafe {
            let next = self.next;
            (*new).next = next;
            (*new).prev = self;
            (*next).prev = new;
            self.next = new;
        }
    }

    /// 从链表中删除
    pub unsafe fn del(&mut self) {
        unsafe {
            let next = self.next;
            let prev = self.prev;
            (*prev).next = next;
            (*next).prev = prev;
            self.next = core::ptr::null_mut();
            self.prev = core::ptr::null_mut();
        }
    }
}

impl Default for ListHead {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 页帧描述符
// ============================================================================

/// 页帧描述符
///
/// 每个物理页帧都有一个对应的 Page 结构，存储该页的元数据
#[repr(C)]
pub struct Page {
    /// 页帧标志
    flags: AtomicU32,

    /// 引用计数
    /// - 0: 空闲页
    /// - >0: 被使用中
    refcount: AtomicU32,

    /// 映射计数（多少页表项指向此页）
    /// - -1: 未映射
    /// - 0: 内核映射
    /// - >0: 用户空间映射数
    mapcount: AtomicI32,

    /// 所属 Zone ID
    zone_id: u8,

    /// Buddy order（如果在空闲链表中）
    order: AtomicU8,

    /// 当前所有权（Buddy/PCP/SLUB/页表等）
    owner: AtomicU8,
    /// 保留字段（对齐）
    _reserved: u8,

    /// 用于链表（Buddy 空闲链表、LRU 等）
    pub lru: ListHead,

    /// 私有数据指针
    /// - SLUB: 指向 KmemCache
    /// - 文件页: 指向 address_space
    private: *mut u8,
}

// Page 可以在多线程间安全共享（通过原子操作）
unsafe impl Send for Page {}
unsafe impl Sync for Page {}

impl Page {
    /// 创建未初始化的 Page
    pub const fn uninit() -> Self {
        Self {
            flags: AtomicU32::new(0),
            refcount: AtomicU32::new(0),
            mapcount: AtomicI32::new(-1),
            zone_id: 0,
            order: AtomicU8::new(0),
            owner: AtomicU8::new(PageOwner::Unknown as u8),
            _reserved: 0,
            lru: ListHead::new(),
            private: core::ptr::null_mut(),
        }
    }

    /// 初始化 Page
    pub fn init(&mut self, zone_id: u8) {
        self.flags.store(0, Ordering::Relaxed);
        self.refcount.store(0, Ordering::Relaxed);
        self.mapcount.store(-1, Ordering::Relaxed);
        self.zone_id = zone_id;
        self.order.store(0, Ordering::Relaxed);
        self.owner
            .store(PageOwner::Unknown as u8, Ordering::Relaxed);
        self.lru.init();
        self.private = core::ptr::null_mut();
    }

    // ========== 标志操作 ==========

    /// 获取标志
    pub fn flags(&self) -> PageFlags {
        PageFlags(self.flags.load(Ordering::Relaxed))
    }

    /// 设置标志位
    pub fn set_flag(&self, flag: u32) {
        self.flags.fetch_or(flag, Ordering::Relaxed);
    }

    /// 清除标志位
    pub fn clear_flag(&self, flag: u32) {
        self.flags.fetch_and(!flag, Ordering::Relaxed);
    }

    /// 测试标志位
    pub fn test_flag(&self, flag: u32) -> bool {
        (self.flags.load(Ordering::Relaxed) & flag) != 0
    }

    // ========== 引用计数 ==========

    /// 获取引用计数
    pub fn refcount(&self) -> u32 {
        self.refcount.load(Ordering::Relaxed)
    }

    /// 增加引用计数
    pub fn get(&self) -> u32 {
        self.refcount.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 减少引用计数，返回新值
    pub fn put(&self) -> u32 {
        match self.try_put() {
            Ok(new) => new,
            Err(PageCounterError::RefUnderflow) => 0,
            Err(PageCounterError::MapcountUnderflow) => 0,
        }
    }

    /// 尝试减少引用计数，失败时返回下溢错误
    pub fn try_put(&self) -> Result<u32, PageCounterError> {
        let prev = self
            .refcount
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                old.checked_sub(1)
            });
        let old = match prev {
            Ok(v) => v,
            Err(_) => {
                PAGE_REF_UNDERFLOW_REJECTS.fetch_add(1, Ordering::Relaxed);
                return Err(PageCounterError::RefUnderflow);
            }
        };
        if old == 0 {
            PAGE_REF_UNDERFLOW_REJECTS.fetch_add(1, Ordering::Relaxed);
            return Err(PageCounterError::RefUnderflow);
        }
        Ok(old - 1)
    }

    /// 设置引用计数为 1（新分配的页）
    pub fn set_count_one(&self) {
        self.refcount.store(1, Ordering::Relaxed);
    }

    // ========== 映射计数 ==========

    /// 获取映射计数
    pub fn mapcount(&self) -> i32 {
        self.mapcount.load(Ordering::Relaxed)
    }

    /// 增加映射计数
    pub fn inc_mapcount(&self) -> i32 {
        self.mapcount.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 减少映射计数
    pub fn dec_mapcount(&self) -> i32 {
        match self.try_dec_mapcount() {
            Ok(new) => new,
            Err(PageCounterError::MapcountUnderflow) => -1,
            Err(PageCounterError::RefUnderflow) => -1,
        }
    }

    /// 尝试减少映射计数，最小值限制为 -1
    pub fn try_dec_mapcount(&self) -> Result<i32, PageCounterError> {
        let prev = self
            .mapcount
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                if old <= -1 {
                    None
                } else {
                    Some(old - 1)
                }
            });
        let old = match prev {
            Ok(v) => v,
            Err(_) => {
                PAGE_MAPCOUNT_UNDERFLOW_REJECTS.fetch_add(1, Ordering::Relaxed);
                return Err(PageCounterError::MapcountUnderflow);
            }
        };
        if old <= -1 {
            PAGE_MAPCOUNT_UNDERFLOW_REJECTS.fetch_add(1, Ordering::Relaxed);
            return Err(PageCounterError::MapcountUnderflow);
        }
        Ok(old - 1)
    }

    // ========== Zone 和 Order ==========

    /// 获取 Zone ID
    pub fn zone_id(&self) -> u8 {
        self.zone_id
    }

    /// 设置 Zone ID
    pub fn set_zone_id(&mut self, id: u8) {
        self.zone_id = id;
    }

    /// 获取 Buddy order
    pub fn order(&self) -> u8 {
        self.order.load(Ordering::Relaxed)
    }

    /// 设置 Buddy order
    pub fn set_order(&mut self, order: u8) {
        self.order.store(order, Ordering::Relaxed);
    }

    #[inline]
    pub fn owner(&self) -> PageOwner {
        PageOwner::from_raw(self.owner.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn set_owner(&self, owner: PageOwner) {
        self.owner.store(owner as u8, Ordering::Relaxed);
    }

    // ========== 私有数据 ==========

    /// 获取私有数据指针
    pub fn private(&self) -> *mut u8 {
        self.private
    }

    /// 设置私有数据指针
    pub fn set_private(&mut self, ptr: *mut u8) {
        self.private = ptr;
    }

    // ========== 状态检查 ==========

    /// 是否为保留页
    pub fn is_reserved(&self) -> bool {
        self.test_flag(PageFlags::RESERVED)
    }

    /// 是否在 Buddy 系统中
    pub fn is_buddy(&self) -> bool {
        self.test_flag(PageFlags::BUDDY)
    }

    /// 是否被 SLUB 使用
    pub fn is_slab(&self) -> bool {
        self.test_flag(PageFlags::SLAB)
    }

    /// 是否为复合页
    pub fn is_compound(&self) -> bool {
        self.test_flag(PageFlags::COMPOUND)
    }

    /// 是否为页表页
    pub fn is_pgtable(&self) -> bool {
        self.test_flag(PageFlags::PGTABLE)
    }

    /// 标记为保留
    pub fn mark_reserved(&self) {
        self.set_flag(PageFlags::RESERVED);
        self.set_owner(PageOwner::Reserved);
    }

    /// 标记为 Buddy
    pub fn mark_buddy(&self, order: u8) {
        self.set_flag(PageFlags::BUDDY);
        self.order.store(order, Ordering::Relaxed);
        self.set_owner(PageOwner::Buddy);
    }

    /// 清除 Buddy 标记
    pub fn clear_buddy(&self) {
        self.clear_flag(PageFlags::BUDDY);
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::uninit()
    }
}

// ============================================================================
// 全局页帧数组
// ============================================================================

/// vmemmap 基地址（所有 Page 结构的数组）
///
/// 在初始化时设置，之后通过此地址访问任意 PFN 的 Page 结构
pub static VMEMMAP_BASE: AtomicUsize = AtomicUsize::new(0);

/// 最大 PFN
pub static MAX_PFN: AtomicU64 = AtomicU64::new(0);

/// 初始化 vmemmap
///
/// # Safety
///
/// - base 必须指向足够大的内存区域
/// - max_pfn 必须正确
pub unsafe fn init_vmemmap(base: *mut Page, max_pfn: u64) {
    VMEMMAP_BASE.store(base as usize, Ordering::Release);
    MAX_PFN.store(max_pfn, Ordering::Release);
    let vmemmap_base = VMEMMAP_BASE.load(Ordering::Acquire);
    let max_pfn_snapshot = MAX_PFN.load(Ordering::Acquire);
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[mm] init_vmemmap base={:#x} max_pfn={} vmemmap_sym={:#x} max_pfn_sym={:#x}",
            vmemmap_base,
            max_pfn_snapshot,
            core::ptr::addr_of!(VMEMMAP_BASE) as usize,
            core::ptr::addr_of!(MAX_PFN) as usize,
        );
    }
}

#[inline]
pub fn vmemmap_base_ptr() -> *mut Page {
    VMEMMAP_BASE.load(Ordering::Acquire) as *mut Page
}

#[inline]
pub fn max_pfn() -> u64 {
    MAX_PFN.load(Ordering::Acquire)
}

/// PFN 转 Page 指针
///
/// # Safety
///
/// - pfn 必须有效
/// - 返回的是原始指针，不提供 Rust 独占可变借用保证
#[inline]
pub unsafe fn pfn_to_page(pfn: u64) -> *mut Page {
    let max = max_pfn();
    let base = vmemmap_base_ptr();
    debug_assert!(pfn < max, "PFN out of range");
    base.add(pfn as usize)
}

/// PFN 转 Page 共享引用
///
/// # Safety
///
/// 调用者需确保 pfn 有效且生命周期内底层 Page 元数据有效。
#[inline]
pub unsafe fn pfn_to_page_ref(pfn: u64) -> &'static Page {
    unsafe { &*pfn_to_page(pfn) }
}

/// PFN 转 Page 可变引用（显式独占语义）
///
/// # Safety
///
/// 调用者必须保证该 Page 在当前时刻不存在其他可变或不可变别名访问。
#[inline]
pub unsafe fn pfn_to_page_mut(pfn: u64) -> &'static mut Page {
    unsafe { &mut *pfn_to_page(pfn) }
}

/// Page 指针转 PFN
#[inline]
pub fn page_to_pfn(page: &Page) -> u64 {
    let base = vmemmap_base_ptr() as usize;
    let ptr = page as *const Page as usize;
    let max_pfn = max_pfn() as usize;
    let page_size = core::mem::size_of::<Page>();

    if base == 0 || max_pfn == 0 || ptr < base {
        return u64::MAX;
    }

    let span_bytes = max_pfn.saturating_mul(page_size);
    let end = base.saturating_add(span_bytes);
    if ptr >= end {
        return u64::MAX;
    }

    let delta = ptr - base;
    if delta % page_size != 0 {
        return u64::MAX;
    }

    (delta / page_size) as u64
}

// ============================================================================
// Page 大小
// ============================================================================

/// Page 结构大小
pub const PAGE_STRUCT_SIZE: usize = core::mem::size_of::<Page>();

// 编译时检查大小
const _: () = assert!(PAGE_STRUCT_SIZE <= 64, "Page struct too large");
