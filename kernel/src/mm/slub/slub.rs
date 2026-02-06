// ============================================================================
// january_os - SLUB 内核对象分配器
//
// 参考 Linux 内核 SLUB 实现，提供高效的小对象分配
// ============================================================================

#![allow(unsafe_op_in_unsafe_fn)]

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use crate::mm::page::page::{Page, PageFlags, pfn_to_page, page_to_pfn};
use crate::mm::page::zone::GfpFlags;
use crate::mm::page::buddy::{alloc_pages, free_pages};
use crate::mm::vm::layout::PAGE_SIZE;

// ============================================================================
// 常量
// ============================================================================

/// kmalloc 大小类数量
pub const KMALLOC_SHIFT_LOW: usize = 3;   // 最小 8 字节
pub const KMALLOC_SHIFT_HIGH: usize = 13; // 最大 8192 字节
pub const KMALLOC_NUM_CACHES: usize = KMALLOC_SHIFT_HIGH - KMALLOC_SHIFT_LOW + 1;

/// 每个 slab 最大对象数
pub const MAX_OBJS_PER_SLAB: usize = 512;

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
    /// 是否已初始化
    pub initialized: bool,
}

impl KmemCache {
    /// 创建未初始化的缓存
    pub const fn uninit() -> Self {
        Self {
            name: "",
            object_size: 0,
            size: 0,
            align: 0,
            objects_per_slab: 0,
            order: 0,
            partial: AtomicPtr::new(core::ptr::null_mut()),
            allocated: AtomicUsize::new(0),
            slabs: AtomicUsize::new(0),
            initialized: false,
        }
    }
    
    /// 初始化缓存
    pub fn init(&mut self, name: &'static str, size: usize, align: usize) {
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
        let mut page_ptr = self.partial.load(Ordering::Acquire);
        
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
        // 分配页
        let page = alloc_pages(self.order, gfp)?;
        
        unsafe {
            // 初始化 slab 元数据
            let slab_meta_size = core::mem::size_of::<SlabPage>();
            let slab = page as *mut Page as *mut u8;
            
            // 将 SlabPage 存储在页的开头（或单独分配）
            // 简化实现：使用 Page.private 指向单独分配的元数据
            // 这里我们在页的末尾保留空间给元数据
            
            let pfn = page_to_pfn(page);
            let page_addr = pfn * PAGE_SIZE;
            let slab_size = PAGE_SIZE * (1 << self.order) as u64;
            
            // 初始化空闲链表
            let obj_start = page_addr as *mut u8;
            let mut prev: *mut FreePointer = core::ptr::null_mut();
            
            for i in (0..self.objects_per_slab).rev() {
                let obj = obj_start.add(i * self.size) as *mut FreePointer;
                (*obj).next = prev;
                prev = obj;
            }
            
            // 设置页标志
            page.set_flag(PageFlags::SLAB);
            
            // 注意：简化实现，不使用单独的 SlabPage 结构
            // 直接使用 Page 的字段
            
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
    fn add_to_partial(&self, page: &mut Page) {
        loop {
            let old = self.partial.load(Ordering::Relaxed);
            // 使用 page.lru 作为链表节点
            unsafe {
                page.lru.next = old as *mut _;
            }
            
            if self.partial.compare_exchange_weak(
                old, 
                page as *mut _, 
                Ordering::Release, 
                Ordering::Relaxed
            ).is_ok() {
                break;
            }
        }
    }
    
    /// 释放对象
    pub unsafe fn free(&self, ptr: *mut u8) {
        if ptr.is_null() || !self.initialized {
            return;
        }
        
        // 找到对象所属的页
        let addr = ptr as u64;
        let pfn = addr / PAGE_SIZE;
        let page = pfn_to_page(pfn);
        
        // 将对象添加回 freelist
        let obj = ptr as *mut FreePointer;
        
        // 使用 page.lru.prev 作为 freelist 头指针
        let free_head = (*page).lru.prev as *mut FreePointer;
        (*obj).next = free_head;
        (*page).lru.prev = obj as *mut _;
        
        self.allocated.fetch_sub(1, Ordering::Relaxed);
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
pub static mut KMALLOC_CACHES: [KmemCache; KMALLOC_NUM_CACHES] = [
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
];

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
    for i in 0..KMALLOC_NUM_CACHES {
        let size = 1 << (KMALLOC_SHIFT_LOW + i);
        KMALLOC_CACHES[i].init(KMALLOC_NAMES[i], size, 8);
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
    
    let index = size.next_power_of_two().trailing_zeros() as usize;
    if index < KMALLOC_SHIFT_LOW {
        return Some(unsafe { &KMALLOC_CACHES[0] });
    }
    
    let cache_index = index - KMALLOC_SHIFT_LOW;
    if cache_index >= KMALLOC_NUM_CACHES {
        return None; // 太大，应该用 alloc_pages
    }
    
    Some(unsafe { &KMALLOC_CACHES[cache_index] })
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
            let pfn = page_to_pfn(page);
            return (pfn * PAGE_SIZE) as *mut u8;
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
    let pfn = addr / PAGE_SIZE;
    let page = pfn_to_page(pfn);
    
    // 检查是否为 slab 页
    if page.is_slab() {
        // 从 slab 释放
        // 需要找到对应的 cache
        // 简化实现：遍历所有 cache
        for i in 0..KMALLOC_NUM_CACHES {
            let cache = &KMALLOC_CACHES[i];
            if cache.initialized {
                // 检查指针是否在这个 cache 的 slab 中
                // 简化：直接尝试释放
                cache.free(ptr);
                return;
            }
        }
    } else {
        // 大分配，释放页
        let order = page.order() as usize;
        free_pages(&mut *page, order);
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 向上对齐
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// SLUB 是否已初始化
pub fn slub_initialized() -> bool {
    unsafe { KMALLOC_CACHES[0].initialized }
}
