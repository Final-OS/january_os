// ============================================================================
// january_os - 内核堆分配器
//
// 支持 Rust 的 alloc crate (Box, Vec, String 等)。
// 分配器基于 Buddy 按需扩展多个线性段，避免固定 1MiB 堆导致的早期 OOM。
// ============================================================================

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

use crate::mm::{self, alloc_pages, free_pages, page_to_pfn, PAGE_SIZE};
use crate::sync::SpinLock;

// ============================================================================
// 堆配置
// ============================================================================

/// 每次增长至少申请 1 MiB，减少小块扩容频率
const HEAP_GROW_MIN: usize = 1024 * 1024;

/// 堆最多拆分成多少段
const MAX_HEAP_SEGMENTS: usize = 64;

#[inline]
const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

#[derive(Clone, Copy)]
struct HeapSegment {
    start: usize,
    end: usize,
    next: usize,
}

impl HeapSegment {
    const fn empty() -> Self {
        Self {
            start: 0,
            end: 0,
            next: 0,
        }
    }

    fn size(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    fn used(&self) -> usize {
        self.next.saturating_sub(self.start)
    }

    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let alloc_start = align_up(self.next, layout.align());
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return null_mut(),
        };
        if alloc_end > self.end {
            return null_mut();
        }
        self.next = alloc_end;
        alloc_start as *mut u8
    }

    fn reset(&mut self) {
        self.next = self.start;
    }
}

#[derive(Clone, Copy)]
struct HeapState {
    initialized: bool,
    segments: [HeapSegment; MAX_HEAP_SEGMENTS],
    segment_count: usize,
    live_allocations: usize,
}

impl HeapState {
    const fn new() -> Self {
        Self {
            initialized: false,
            segments: [HeapSegment::empty(); MAX_HEAP_SEGMENTS],
            segment_count: 0,
            live_allocations: 0,
        }
    }

    fn total_size(&self) -> usize {
        let mut total = 0usize;
        let mut i = 0usize;
        while i < self.segment_count {
            total = total.saturating_add(self.segments[i].size());
            i += 1;
        }
        total
    }

    fn used_size(&self) -> usize {
        let mut used = 0usize;
        let mut i = 0usize;
        while i < self.segment_count {
            used = used.saturating_add(self.segments[i].used());
            i += 1;
        }
        used
    }

    fn free_size(&self) -> usize {
        self.total_size().saturating_sub(self.used_size())
    }
}

#[derive(Clone, Copy)]
pub struct HeapStats {
    pub initialized: bool,
    pub segments: usize,
    pub total_size: usize,
    pub used_size: usize,
    pub free_size: usize,
    pub live_allocations: usize,
}

static HEAP_STATE: SpinLock<HeapState> = SpinLock::with_name(HeapState::new(), "KernelHeapState");

fn max_chunk_order() -> usize {
    mm::MAX_ORDER.saturating_sub(1)
}

fn max_chunk_size() -> usize {
    (1usize << max_chunk_order()) * PAGE_SIZE as usize
}

fn order_for_size(bytes: usize) -> Option<usize> {
    if bytes == 0 {
        return Some(0);
    }
    let page_size = PAGE_SIZE as usize;
    let pages = bytes.checked_add(page_size - 1)?.checked_div(page_size)?;
    if pages == 0 {
        return Some(0);
    }
    let order = mm::get_order(pages as u64);
    if order >= mm::MAX_ORDER {
        None
    } else {
        Some(order)
    }
}

fn append_segment_locked(state: &mut HeapState, segment: HeapSegment) -> bool {
    if state.segment_count >= MAX_HEAP_SEGMENTS {
        return false;
    }
    state.segments[state.segment_count] = segment;
    state.segment_count += 1;
    true
}

struct SegmentReservation {
    page: *mut mm::Page,
    order: usize,
    segment: HeapSegment,
}

unsafe fn reserve_segment(min_bytes: usize) -> Option<SegmentReservation> {
    let request = min_bytes.max(PAGE_SIZE as usize);
    let order = order_for_size(request)?;
    let page = alloc_pages(order, mm::GFP_KERNEL)?;
    let phys = page_to_pfn(page) * PAGE_SIZE;
    let start = mm::phys_to_virt(phys) as usize;
    let size = (1usize << order) * PAGE_SIZE as usize;
    let end = start.checked_add(size)?;

    Some(SegmentReservation {
        page: page as *mut mm::Page,
        order,
        segment: HeapSegment {
            start,
            end,
            next: start,
        },
    })
}

unsafe fn release_reservation(reservation: SegmentReservation) {
    unsafe {
        free_pages(&mut *reservation.page, reservation.order);
    }
}

fn grow_target_for(min_alloc_size: usize) -> Option<usize> {
    let max_chunk = max_chunk_size();
    if min_alloc_size > max_chunk {
        return None;
    }
    Some(min_alloc_size.max(HEAP_GROW_MIN).min(max_chunk))
}

unsafe fn grow_heap_once(min_alloc_size: usize) -> bool {
    let grow_target = match grow_target_for(min_alloc_size) {
        Some(v) => v,
        None => return false,
    };
    let reservation = match unsafe { reserve_segment(grow_target) } {
        Some(v) => v,
        None => return false,
    };

    let mut state = HEAP_STATE.lock();
    if !state.initialized || !append_segment_locked(&mut state, reservation.segment) {
        drop(state);
        unsafe { release_reservation(reservation) };
        return false;
    }
    true
}

struct SimpleHeap;

unsafe impl GlobalAlloc for SimpleHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return layout.align() as *mut u8;
        }

        let required = match layout.size().checked_add(layout.align().saturating_sub(1)) {
            Some(v) => v,
            None => return null_mut(),
        };

        loop {
            let mut state = HEAP_STATE.lock();
            if !state.initialized {
                return null_mut();
            }

            let mut i = 0usize;
            while i < state.segment_count {
                let ptr = state.segments[i].alloc(layout);
                if !ptr.is_null() {
                    state.live_allocations = state.live_allocations.saturating_add(1);
                    return ptr;
                }
                i += 1;
            }
            drop(state);

            if unsafe { !grow_heap_once(required) } {
                return null_mut();
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            return;
        }

        let mut state = HEAP_STATE.lock();
        if state.live_allocations == 0 {
            return;
        }
        state.live_allocations -= 1;
        if state.live_allocations == 0 {
            let mut i = 0usize;
            while i < state.segment_count {
                state.segments[i].reset();
                i += 1;
            }
        }
    }
}

/// `Box/Vec/String` 全局分配器：优先走 SLUB kmalloc/kfree。
///
/// 使用头部记录原始 kmalloc 指针，以支持任意对齐请求。
struct KernelGlobalAllocator;

const GLOBAL_ALLOC_MAGIC: usize = 0x4A4F_5348_4541_5032; // "JOSHEAP2"
const GLOBAL_ALLOC_HEADER_WORDS: usize = 3;

#[inline]
unsafe fn kmalloc_aligned(layout: Layout) -> *mut u8 {
    let header_size = core::mem::size_of::<usize>() * GLOBAL_ALLOC_HEADER_WORDS;
    let align_pad = layout.align().saturating_sub(1);
    let request = match layout
        .size()
        .checked_add(align_pad)
        .and_then(|v| v.checked_add(header_size))
    {
        Some(v) => v,
        None => return null_mut(),
    };

    let raw = crate::mm::slub::kmalloc(request, crate::mm::GFP_KERNEL);
    if raw.is_null() {
        return null_mut();
    }

    let raw_addr = raw as usize;
    let user_addr = align_up(raw_addr.saturating_add(header_size), layout.align());
    let header_addr = user_addr.saturating_sub(header_size);
    let header_ptr = header_addr as *mut usize;
    let checksum = raw_addr ^ user_addr ^ GLOBAL_ALLOC_MAGIC;

    // 使用非对齐写，避免对头部地址做额外对齐约束。
    core::ptr::write_unaligned(header_ptr, GLOBAL_ALLOC_MAGIC);
    core::ptr::write_unaligned(header_ptr.add(1), raw_addr);
    core::ptr::write_unaligned(header_ptr.add(2), checksum);

    user_addr as *mut u8
}

#[inline]
unsafe fn kfree_aligned(ptr: *mut u8, layout: Layout) -> bool {
    let header_size = core::mem::size_of::<usize>() * GLOBAL_ALLOC_HEADER_WORDS;
    let user_addr = ptr as usize;
    if user_addr < header_size {
        return false;
    }

    let header_addr = user_addr - header_size;
    let header_ptr = header_addr as *const usize;
    let magic = core::ptr::read_unaligned(header_ptr);
    if magic != GLOBAL_ALLOC_MAGIC {
        return false;
    }

    let raw_addr = core::ptr::read_unaligned(header_ptr.add(1));
    let checksum = core::ptr::read_unaligned(header_ptr.add(2));
    if raw_addr == 0
        || raw_addr > user_addr
        || !is_direct_map_ptr(raw_addr)
        || checksum != (raw_addr ^ user_addr ^ GLOBAL_ALLOC_MAGIC)
    {
        return false;
    }

    let expected_user = align_up(
        raw_addr.saturating_add(header_size),
        layout.align(),
    );
    if expected_user != user_addr {
        return false;
    }

    unsafe {
        crate::mm::slub::kfree(raw_addr as *mut u8);
    }
    true
}

#[inline]
fn is_direct_map_ptr(addr: usize) -> bool {
    let base = mm::direct_map_offset() as usize;
    if addr < base {
        return false;
    }
    let phys = addr - base;
    let max_phys = mm::max_pfn().saturating_mul(PAGE_SIZE) as usize;
    max_phys != 0 && phys < max_phys
}

unsafe impl GlobalAlloc for KernelGlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return layout.align() as *mut u8;
        }

        if crate::mm::slub::slub_initialized() {
            return unsafe { kmalloc_aligned(layout) };
        }

        // 仅早期阶段回退到简单堆；SLUB 就绪后统一走 kmalloc 路径。
        unsafe { HEAP_ALLOCATOR.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || layout.size() == 0 {
            return;
        }

        if unsafe { kfree_aligned(ptr, layout) } {
            return;
        }

        unsafe { HEAP_ALLOCATOR.dealloc(ptr, layout) };
    }
}

// ============================================================================
// 全局分配器
// ============================================================================

pub static HEAP_ALLOCATOR: SimpleHeap = SimpleHeap;

#[global_allocator]
static GLOBAL_ALLOCATOR: KernelGlobalAllocator = KernelGlobalAllocator;

/// 直接通过全局堆分配指定布局（用于内核自测）
pub unsafe fn heap_alloc_raw(layout: Layout) -> *mut u8 {
    unsafe { HEAP_ALLOCATOR.alloc(layout) }
}

/// 直接通过全局堆释放指定布局（用于内核自测）
pub unsafe fn heap_dealloc_raw(ptr: *mut u8, layout: Layout) {
    unsafe { HEAP_ALLOCATOR.dealloc(ptr, layout) }
}

/// 初始化并预热内核堆
///
/// `target_size` 为期望初始容量；分配器会分段申请，尽量接近该值。
///
/// # Returns
/// 返回已成功预热的总堆容量（字节）
pub unsafe fn init_heap(target_size: usize) -> usize {
    {
        let mut state = HEAP_STATE.lock();
        if state.initialized {
            return state.total_size();
        }
        state.initialized = true;
    }

    let mut remain = target_size.max(PAGE_SIZE as usize);
    let max_chunk = max_chunk_size();
    let page_size = PAGE_SIZE as usize;
    let mut request = remain.min(max_chunk).max(page_size);

    while remain > 0 {
        let reservation = unsafe { reserve_segment(request) };
        if let Some(resv) = reservation {
            let added = resv.segment.size();
            let mut inserted = false;
            {
                let mut state = HEAP_STATE.lock();
                if append_segment_locked(&mut state, resv.segment) {
                    inserted = true;
                }
            }
            if inserted {
                remain = remain.saturating_sub(added);
                request = remain.min(max_chunk).max(page_size);
            } else {
                unsafe { release_reservation(resv) };
                break;
            }
        } else if request > page_size {
            request /= 2;
        } else {
            break;
        }
    }

    HEAP_STATE.lock().total_size()
}

/// 获取堆统计信息
pub fn heap_stats() -> HeapStats {
    let state = HEAP_STATE.lock();
    HeapStats {
        initialized: state.initialized,
        segments: state.segment_count,
        total_size: state.total_size(),
        used_size: state.used_size(),
        free_size: state.free_size(),
        live_allocations: state.live_allocations,
    }
}

// ============================================================================
// OOM 处理
// ============================================================================

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("Allocation error: {:?}", layout)
}
