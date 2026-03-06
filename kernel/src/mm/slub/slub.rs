// ============================================================================
// january_os - SLUB 内核对象分配器
//
// 参考 Linux 内核 SLUB 实现，提供高效的小对象分配
// ============================================================================

#![allow(unsafe_op_in_unsafe_fn)]

use crate::mm::page::buddy::{alloc_pages, free_pages};
use crate::mm::page::page::{max_pfn, page_to_pfn, pfn_to_page, Page, PageFlags, PageOwner};
use crate::mm::page::zone::GfpFlags;
use crate::mm::vm::layout::PAGE_SIZE;
use crate::sync::IrqSpinLock;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

// ============================================================================
// 常量
// ============================================================================

/// kmalloc 大小类数量
pub const KMALLOC_SHIFT_LOW: usize = 3; // 最小 8 字节
pub const KMALLOC_SHIFT_HIGH: usize = 13; // 最大 8192 字节
pub const KMALLOC_NUM_CACHES: usize = KMALLOC_SHIFT_HIGH - KMALLOC_SHIFT_LOW + 1;

/// 每个 slab 最大对象数
pub const MAX_OBJS_PER_SLAB: usize = 512;
const KMEM_CACHE_MAGIC: u64 = 0x4a4f_534c_5542_4348;

#[derive(Clone, Copy)]
pub struct KmallocCacheStats {
    pub name: &'static str,
    pub object_size: usize,
    pub allocated_objects: usize,
    pub slabs: usize,
    pub allocated_bytes: usize,
    pub slab_bytes: usize,
}

impl KmallocCacheStats {
    const fn empty() -> Self {
        Self {
            name: "",
            object_size: 0,
            allocated_objects: 0,
            slabs: 0,
            allocated_bytes: 0,
            slab_bytes: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct KmallocStats {
    pub initialized: bool,
    pub active_caches: usize,
    pub total_allocated_objects: usize,
    pub total_allocated_bytes: usize,
    pub total_slabs: usize,
    pub total_slab_bytes: usize,
    pub large_allocations: usize,
    pub large_alloc_pages: usize,
    pub caches: [KmallocCacheStats; KMALLOC_NUM_CACHES],
}

// ============================================================================
// Slab 页管理
// ============================================================================

/// Slab 页元数据（存储在 Page.private 中）
#[repr(C)]
pub struct SlabPage {
    /// 空闲对象链表（指向第一个空闲对象）
    pub freelist: *mut FreePointer,
    /// 正在使用的对象数
    pub inuse: u16,
    /// 总对象数
    pub objects: u16,
    /// 所属 KmemCache
    pub cache: *mut KmemCache,
}

/// 空闲对象指针（嵌入在空闲对象中）
#[repr(C)]
pub struct FreePointer {
    pub next: *mut FreePointer,
}

// ============================================================================
// KmemCache
// ============================================================================

/// 内核内存缓存
///
/// 管理特定大小对象的分配
pub struct KmemCache {
    /// 校验标记，防止 page.private 损坏后误解引用
    pub magic: u64,
    /// 缓存名称
    pub name: &'static str,
    /// 对象大小
    pub object_size: usize,
    /// 对齐后的对象大小
    pub size: usize,
    /// 对齐要求
    pub align: usize,
    /// 每个 slab 的对象数
    pub objects_per_slab: usize,
    /// slab 需要的页数 (2^order)
    pub order: usize,
    /// 部分空闲 slab 链表
    pub partial: AtomicPtr<Page>,
    /// 分配的对象数
    pub allocated: AtomicUsize,
    /// 分配的 slab 数
    pub slabs: AtomicUsize,
    /// 保护 partial 链表和 freelist 操作
    pub lock: IrqSpinLock<()>,
    /// 是否已初始化
    pub initialized: bool,
}

impl KmemCache {
    /// 创建未初始化的缓存
    pub const fn uninit() -> Self {
        Self {
            magic: 0,
            name: "",
            object_size: 0,
            size: 0,
            align: 0,
            objects_per_slab: 0,
            order: 0,
            partial: AtomicPtr::new(core::ptr::null_mut()),
            allocated: AtomicUsize::new(0),
            slabs: AtomicUsize::new(0),
            lock: IrqSpinLock::new(()),
            initialized: false,
        }
    }

    /// 初始化缓存
    pub fn init(&mut self, name: &'static str, size: usize, align: usize) {
        self.magic = KMEM_CACHE_MAGIC;
        self.name = name;
        self.object_size = size;
        self.align = align.max(core::mem::size_of::<FreePointer>());

        // 对齐后的大小（至少能容纳 FreePointer）
        self.size = align_up(size.max(core::mem::size_of::<FreePointer>()), self.align);

        // 计算需要的 order 和每个 slab 的对象数
        self.calculate_order();

        self.initialized = true;
    }

    /// 计算最佳 order
    fn calculate_order(&mut self) {
        // 从 order 0 开始尝试
        for order in 0..11 {
            let slab_size = PAGE_SIZE as usize * (1 << order);
            let objects = slab_size / self.size;

            if objects >= 4 && objects <= MAX_OBJS_PER_SLAB {
                self.order = order;
                self.objects_per_slab = objects;
                return;
            }
        }

        // 默认使用 order 0
        self.order = 0;
        self.objects_per_slab = (PAGE_SIZE as usize / self.size).max(1);
    }

    /// 从缓存分配对象
    pub fn alloc(&self, gfp: GfpFlags) -> *mut u8 {
        if !self.initialized {
            return core::ptr::null_mut();
        }

        // 1. 尝试从 partial slab 分配
        if let Some(ptr) = self.alloc_from_partial() {
            return ptr;
        }

        // 2. 分配新的 slab
        if let Some(ptr) = self.alloc_new_slab(gfp) {
            return ptr;
        }

        core::ptr::null_mut()
    }

    /// 从 partial slab 分配
    fn alloc_from_partial(&self) -> Option<*mut u8> {
        let _guard = self.lock.lock();
        let mut page_ptr = self.partial.load(Ordering::Relaxed);

        while !page_ptr.is_null() {
            unsafe {
                let page = &mut *page_ptr;

                // 使用 page.lru.prev 存储 freelist 头指针
                let free_head = page.lru.prev as *mut FreePointer;

                if !free_head.is_null() {
                    // 从 freelist 取出一个对象
                    let next_free = (*free_head).next;
                    page.lru.prev = next_free as *mut _;

                    self.allocated.fetch_add(1, Ordering::Relaxed);

                    return Some(free_head as *mut u8);
                }

                // 尝试下一个 page
                // page.lru.next 存储下一个 page 的指针
                page_ptr = page.lru.next as *mut Page;
            }
        }

        None
    }

    /// 分配新的 slab
    fn alloc_new_slab(&self, gfp: GfpFlags) -> Option<*mut u8> {
        // 分配页（在锁外进行，避免持锁调用 buddy）
        let page = alloc_pages(self.order, gfp)?;

        let _guard = self.lock.lock();

        unsafe {
            let pfn = page_to_pfn(page);
            let page_phys = pfn * PAGE_SIZE;
            let page_virt = phys_to_virt(page_phys);

            // 初始化空闲链表
            let obj_start = page_virt as *mut u8;
            let mut prev: *mut FreePointer = core::ptr::null_mut();

            for i in (0..self.objects_per_slab).rev() {
                let obj = obj_start.add(i * self.size) as *mut FreePointer;
                (*obj).next = prev;
                prev = obj;
            }

            // 设置页标志
            page.set_flag(PageFlags::SLAB);
            page.set_owner(PageOwner::Slab);

            // 将 cache 指针存入 page.private，用于 kfree 时定位正确的 cache
            page.set_private(self as *const KmemCache as *mut u8);

            // 取出第一个对象
            let first_obj = prev;
            let second_obj = if !first_obj.is_null() {
                (*first_obj).next
            } else {
                core::ptr::null_mut()
            };

            // 使用 page.lru.prev 存储 freelist 头指针
            (*page).lru.prev = second_obj as *mut _;

            // 添加到 partial 链表
            self.add_to_partial(page);

            self.slabs.fetch_add(1, Ordering::Relaxed);
            self.allocated.fetch_add(1, Ordering::Relaxed);

            Some(first_obj as *mut u8)
        }
    }

    /// 添加 slab 到 partial 链表
    ///
    /// 调用者必须持有 self.lock
    fn add_to_partial(&self, page: &mut Page) {
        let old = self.partial.load(Ordering::Relaxed);
        unsafe {
            page.lru.next = old as *mut _;
        }
        self.partial.store(page as *mut _, Ordering::Relaxed);
    }

    /// 从 partial 链表中摘除一个 slab 页
    ///
    /// 调用者必须持有 `self.lock`。
    fn remove_from_partial(&self, target: *mut Page) -> bool {
        let mut prev: *mut Page = core::ptr::null_mut();
        let mut curr = self.partial.load(Ordering::Relaxed);

        while !curr.is_null() {
            unsafe {
                let next = (*curr).lru.next as *mut Page;
                if curr == target {
                    if prev.is_null() {
                        self.partial.store(next, Ordering::Relaxed);
                    } else {
                        (*prev).lru.next = next as *mut _;
                    }
                    (*curr).lru.next = core::ptr::null_mut();
                    return true;
                }
                prev = curr;
                curr = next;
            }
        }

        false
    }

    /// 统计某个 slab 页当前空闲对象数。
    ///
    /// 调用者必须持有 `self.lock`，以保证 freelist 不被并发修改。
    fn free_objects_in_page(&self, page: *mut Page) -> usize {
        let mut count = 0usize;
        unsafe {
            let mut node = (*page).lru.prev as *mut FreePointer;
            while !node.is_null() {
                count = count.saturating_add(1);
                if count > self.objects_per_slab {
                    break;
                }
                node = (*node).next;
            }
        }
        count
    }

    /// 释放对象
    pub unsafe fn free(&self, ptr: *mut u8) {
        if ptr.is_null() || !self.initialized {
            return;
        }

        // 找到对象所属的页
        let addr = ptr as u64;
        let phys = match virt_to_phys_direct_map(addr) {
            Some(p) => p,
            None => return,
        };
        let pfn = phys / PAGE_SIZE;
        let page = pfn_to_page(pfn);

        let mut release_slab = false;
        {
            let _guard = self.lock.lock();

            // 将对象添加回 freelist
            let obj = ptr as *mut FreePointer;

            // 使用 page.lru.prev 作为 freelist 头指针
            let free_head = (*page).lru.prev as *mut FreePointer;
            (*obj).next = free_head;
            (*page).lru.prev = obj as *mut _;

            self.allocated.fetch_sub(1, Ordering::Relaxed);

            // 整页对象全部归还后，回收 slab 到 buddy。
            if self.free_objects_in_page(page) == self.objects_per_slab
                && self.remove_from_partial(page)
            {
                (*page).lru.prev = core::ptr::null_mut();
                (*page).clear_flag(PageFlags::SLAB);
                (*page).set_owner(PageOwner::Allocated);
                (*page).set_private(core::ptr::null_mut());
                atomic_saturating_sub(&self.slabs, 1);
                release_slab = true;
            }
        }

        if release_slab {
            free_pages(&mut *page, self.order);
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> (usize, usize) {
        (
            self.allocated.load(Ordering::Relaxed),
            self.slabs.load(Ordering::Relaxed),
        )
    }
}

// ============================================================================
// kmalloc 缓存
// ============================================================================

/// kmalloc 大小类缓存
struct KmallocCaches {
    inner: UnsafeCell<[KmemCache; KMALLOC_NUM_CACHES]>,
}

unsafe impl Sync for KmallocCaches {}

impl KmallocCaches {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new([
                KmemCache::uninit(), // 8
                KmemCache::uninit(), // 16
                KmemCache::uninit(), // 32
                KmemCache::uninit(), // 64
                KmemCache::uninit(), // 128
                KmemCache::uninit(), // 256
                KmemCache::uninit(), // 512
                KmemCache::uninit(), // 1024
                KmemCache::uninit(), // 2048
                KmemCache::uninit(), // 4096
                KmemCache::uninit(), // 8192
            ]),
        }
    }
}

static KMALLOC_CACHES: KmallocCaches = KmallocCaches::new();
static LARGE_ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static LARGE_ALLOC_PAGES: AtomicUsize = AtomicUsize::new(0);

/// kmalloc 大小类名称
const KMALLOC_NAMES: [&str; KMALLOC_NUM_CACHES] = [
    "kmalloc-8",
    "kmalloc-16",
    "kmalloc-32",
    "kmalloc-64",
    "kmalloc-128",
    "kmalloc-256",
    "kmalloc-512",
    "kmalloc-1024",
    "kmalloc-2048",
    "kmalloc-4096",
    "kmalloc-8192",
];

/// 初始化 kmalloc 缓存
pub unsafe fn init_kmalloc_caches() {
    let caches = kmalloc_caches_mut();
    for i in 0..KMALLOC_NUM_CACHES {
        let size = 1 << (KMALLOC_SHIFT_LOW + i);
        caches[i].init(KMALLOC_NAMES[i], size, 8);
    }
}

/// 根据大小找到合适的 kmalloc 缓存
fn find_kmalloc_cache(size: usize) -> Option<&'static KmemCache> {
    if size == 0 {
        return None;
    }

    // 找到能容纳 size 的最小缓存
    let min_size = 1 << KMALLOC_SHIFT_LOW;
    let size = size.max(min_size);

    let caches = kmalloc_caches_ref();

    let index = size.next_power_of_two().trailing_zeros() as usize;
    if index < KMALLOC_SHIFT_LOW {
        return Some(&caches[0]);
    }

    let cache_index = index - KMALLOC_SHIFT_LOW;
    if cache_index >= KMALLOC_NUM_CACHES {
        return None; // 太大，应该用 alloc_pages
    }

    Some(&caches[cache_index])
}

// ============================================================================
// 公共 API
// ============================================================================

/// 分配内核内存
///
/// # Arguments
/// * `size` - 请求的大小（字节）
/// * `gfp` - 分配标志
///
/// # Returns
/// 成功返回内存指针，失败返回空指针
pub fn kmalloc(size: usize, gfp: GfpFlags) -> *mut u8 {
    // 大分配使用 alloc_pages
    if size > (1 << KMALLOC_SHIFT_HIGH) {
        let order = crate::mm::page::zone::get_order((size as u64 + PAGE_SIZE - 1) / PAGE_SIZE);
        if let Some(page) = alloc_pages(order, gfp) {
            LARGE_ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            LARGE_ALLOC_PAGES.fetch_add(pages_for_order(order), Ordering::Relaxed);
            let pfn = page_to_pfn(page);
            let phys = pfn * PAGE_SIZE;
            return phys_to_virt(phys) as *mut u8;
        }
        return core::ptr::null_mut();
    }

    // 小分配使用 SLUB
    match find_kmalloc_cache(size) {
        Some(cache) => cache.alloc(gfp),
        None => core::ptr::null_mut(),
    }
}

/// 分配并清零
pub fn kzalloc(size: usize, gfp: GfpFlags) -> *mut u8 {
    let ptr = kmalloc(size, GfpFlags::new(gfp.bits() | GfpFlags::ZERO));
    if !ptr.is_null() {
        unsafe {
            core::ptr::write_bytes(ptr, 0, size);
        }
    }
    ptr
}

/// 释放内核内存
pub unsafe fn kfree(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    // 找到对应的页
    let addr = ptr as u64;
    let phys = match virt_to_phys_direct_map(addr) {
        Some(p) => p,
        None => {
            crate::warn!(
                "slub: kfree ignored non direct-map pointer addr={:#x}",
                addr
            );
            return;
        }
    };
    let pfn = phys / PAGE_SIZE;
    let page = &mut *pfn_to_page(pfn);

    // 检查是否为 slab 页
    if page.is_slab() {
        if page.owner() != PageOwner::Slab {
            crate::warn!(
                "slub: slab page owner mismatch pfn={} owner={:?}",
                pfn,
                page.owner()
            );
            return;
        }
        if let Some(cache) = resolve_kmalloc_cache_from_page(page) {
            cache.free(ptr);
        } else {
            crate::warn!(
                "slub: kfree ignored slab page with invalid owner pfn={}",
                pfn
            );
        }
    } else {
        if addr & (PAGE_SIZE - 1) != 0 {
            crate::warn!(
                "slub: kfree ignored non-page-aligned large allocation addr={:#x}",
                addr
            );
            return;
        }
        // 大分配，释放页
        let order = page.order() as usize;
        atomic_saturating_sub(&LARGE_ALLOC_COUNT, 1);
        atomic_saturating_sub(&LARGE_ALLOC_PAGES, pages_for_order(order));
        free_pages(&mut *page, order);
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    crate::mm::phys_to_virt(phys)
}

#[inline]
fn virt_to_phys_direct_map(virt: u64) -> Option<u64> {
    let phys = crate::mm::virt_to_phys(virt);
    if phys == 0 && virt < crate::mm::direct_map_offset() {
        return None;
    }
    let max_phys = max_pfn().saturating_mul(PAGE_SIZE);
    if max_phys == 0 || phys >= max_phys {
        None
    } else {
        Some(phys)
    }
}

fn resolve_kmalloc_cache_from_page(page: &Page) -> Option<&'static KmemCache> {
    let cache_ptr = page.private() as *const KmemCache;
    if cache_ptr.is_null() {
        return None;
    }

    let cache_addr = cache_ptr as usize;
    if cache_addr % core::mem::align_of::<KmemCache>() != 0 {
        return None;
    }

    let caches = kmalloc_caches_ref();
    let kmalloc_base = caches.as_ptr() as usize;
    let kmalloc_end = kmalloc_base + core::mem::size_of_val(caches);
    if cache_addr < kmalloc_base || cache_addr >= kmalloc_end {
        return None;
    }

    let cache_size = core::mem::size_of::<KmemCache>();
    if (cache_addr - kmalloc_base) % cache_size != 0 {
        return None;
    }

    let cache = unsafe { &*cache_ptr };
    if cache.magic != KMEM_CACHE_MAGIC || !cache.initialized {
        return None;
    }

    Some(cache)
}

/// 向上对齐
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// SLUB 是否已初始化
pub fn slub_initialized() -> bool {
    kmalloc_caches_ref()[0].initialized
}

/// 获取 kmalloc/SLUB 统计信息，用于 `mm status` 统一展示分配器状态。
pub fn kmalloc_stats() -> KmallocStats {
    let caches = kmalloc_caches_ref();
    let mut stats = KmallocStats {
        initialized: slub_initialized(),
        active_caches: 0,
        total_allocated_objects: 0,
        total_allocated_bytes: 0,
        total_slabs: 0,
        total_slab_bytes: 0,
        large_allocations: LARGE_ALLOC_COUNT.load(Ordering::Relaxed),
        large_alloc_pages: LARGE_ALLOC_PAGES.load(Ordering::Relaxed),
        caches: [KmallocCacheStats::empty(); KMALLOC_NUM_CACHES],
    };

    let mut i = 0usize;
    while i < KMALLOC_NUM_CACHES {
        let cache = &caches[i];
        if cache.initialized {
            let (allocated, slabs) = cache.stats();
            let slab_bytes = slabs
                .saturating_mul(pages_for_order(cache.order).saturating_mul(PAGE_SIZE as usize));
            let allocated_bytes = allocated.saturating_mul(cache.object_size);
            if allocated > 0 || slabs > 0 {
                stats.active_caches += 1;
            }
            stats.total_allocated_objects = stats.total_allocated_objects.saturating_add(allocated);
            stats.total_allocated_bytes =
                stats.total_allocated_bytes.saturating_add(allocated_bytes);
            stats.total_slabs = stats.total_slabs.saturating_add(slabs);
            stats.total_slab_bytes = stats.total_slab_bytes.saturating_add(slab_bytes);
            stats.caches[i] = KmallocCacheStats {
                name: cache.name,
                object_size: cache.object_size,
                allocated_objects: allocated,
                slabs,
                allocated_bytes,
                slab_bytes,
            };
        }
        i += 1;
    }

    stats
}

fn atomic_saturating_sub(target: &AtomicUsize, value: usize) {
    let mut current = target.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(value);
        match target.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

#[inline]
fn pages_for_order(order: usize) -> usize {
    1usize.checked_shl(order as u32).unwrap_or(0)
}

#[inline]
fn kmalloc_caches_ref() -> &'static [KmemCache; KMALLOC_NUM_CACHES] {
    unsafe { &*KMALLOC_CACHES.inner.get() }
}

#[inline]
unsafe fn kmalloc_caches_mut() -> &'static mut [KmemCache; KMALLOC_NUM_CACHES] {
    unsafe { &mut *KMALLOC_CACHES.inner.get() }
}
