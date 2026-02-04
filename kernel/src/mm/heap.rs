// ============================================================================
// january_os - 内核堆分配器
//
// 实现全局堆分配器，支持 Rust 的 alloc crate (Box, Vec, String 等)
// 注意：这个堆分配器是在 Buddy 系统之上工作的简化分配器
// ============================================================================

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// ============================================================================
// 堆配置
// ============================================================================

/// 默认堆大小 (1 MB)
pub const HEAP_SIZE: usize = 1024 * 1024;

// ============================================================================
// 对齐辅助函数
// ============================================================================

/// 向上对齐
const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

// ============================================================================
// 简化堆分配器
// ============================================================================

/// 简化的堆分配器
/// 
/// 使用原子操作实现线程安全的线性分配。
/// 主要用于 `alloc` crate 的全局分配器接口 (Box, Vec 等)。
/// 
/// 注意：这个分配器使用线性分配策略，只有当所有分配都被释放时
/// 才会重置堆。对于长时间运行的内核，考虑使用更高级的分配器。
struct SimpleHeap {
    initialized: AtomicBool,
    heap_start: AtomicUsize,
    heap_end: AtomicUsize,
    next: AtomicUsize,
    allocations: AtomicUsize,
}

unsafe impl Sync for SimpleHeap {}

impl SimpleHeap {
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            heap_start: AtomicUsize::new(0),
            heap_end: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
        }
    }
    
    /// 初始化堆
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        self.heap_start.store(heap_start, Ordering::SeqCst);
        self.heap_end.store(heap_start + heap_size, Ordering::SeqCst);
        self.next.store(heap_start, Ordering::SeqCst);
        self.allocations.store(0, Ordering::SeqCst);
        self.initialized.store(true, Ordering::SeqCst);
    }
    
    pub fn allocated(&self) -> usize {
        self.next.load(Ordering::SeqCst) - self.heap_start.load(Ordering::SeqCst)
    }
    
    pub fn heap_size(&self) -> usize {
        self.heap_end.load(Ordering::SeqCst) - self.heap_start.load(Ordering::SeqCst)
    }
    
    pub fn free_space(&self) -> usize {
        self.heap_end.load(Ordering::SeqCst) - self.next.load(Ordering::SeqCst)
    }
}

unsafe impl GlobalAlloc for SimpleHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.initialized.load(Ordering::SeqCst) {
            return null_mut();
        }
        
        loop {
            let current = self.next.load(Ordering::SeqCst);
            let alloc_start = align_up(current, layout.align());
            let alloc_end = match alloc_start.checked_add(layout.size()) {
                Some(end) => end,
                None => return null_mut(),
            };
            
            if alloc_end > self.heap_end.load(Ordering::SeqCst) {
                return null_mut();
            }
            
            // 尝试 CAS 更新 next
            if self.next.compare_exchange(
                current, alloc_end, Ordering::SeqCst, Ordering::SeqCst
            ).is_ok() {
                self.allocations.fetch_add(1, Ordering::SeqCst);
                return alloc_start as *mut u8;
            }
            // CAS 失败，重试
        }
    }
    
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let prev = self.allocations.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            // 所有分配都被释放，重置堆
            let start = self.heap_start.load(Ordering::SeqCst);
            self.next.store(start, Ordering::SeqCst);
        }
    }
}

// ============================================================================
// 全局分配器
// ============================================================================

/// 全局内核堆分配器
#[global_allocator]
pub static HEAP_ALLOCATOR: SimpleHeap = SimpleHeap::new();

/// 初始化内核堆
///
/// 内核堆用于支持 Rust 的 alloc crate (Box, Vec, String 等)。
/// 必须在使用任何堆分配之前调用此函数。
///
/// # Arguments
/// * `heap_start` - 堆起始虚拟地址
/// * `heap_size` - 堆大小（字节）
///
/// # Safety
/// 
/// - heap_start 必须是有效的、可写的虚拟地址
/// - [heap_start, heap_start + heap_size) 范围内的内存必须已映射且未被使用
/// - 只能调用一次
pub unsafe fn init_heap(heap_start: usize, heap_size: usize) {
    unsafe { HEAP_ALLOCATOR.init(heap_start, heap_size); }
}

/// 获取堆统计信息
///
/// # Returns
/// (总大小, 已分配, 空闲)
pub fn heap_stats() -> (usize, usize, usize) {
    (
        HEAP_ALLOCATOR.heap_size(),
        HEAP_ALLOCATOR.allocated(),
        HEAP_ALLOCATOR.free_space(),
    )
}

// ============================================================================
// OOM 处理
// ============================================================================

/// 内存分配失败处理
#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("Allocation error: {:?}", layout)
}
