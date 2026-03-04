// ============================================================================
// january_os - Buddy System 物理页帧分配器
//
// 参考 Linux 内核实现，基于 Zone 和 struct page
// ============================================================================

use super::page::{max_pfn, page_to_pfn, pfn_to_page, vmemmap_base_ptr, Page, PageFlags};
use super::pcp::{pcp_alloc_page, pcp_free_page, pcp_initialized};
use super::zone::{get_buddy_pfn, pages_per_order};
use super::zone::{get_zone, gfp_to_zone_list, GfpFlags, Zone, MAX_ORDER};
use crate::mm::vm::layout::{DIRECT_MAP_OFFSET, PAGE_SIZE};
use core::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// container_of 宏
// ============================================================================

/// 从成员指针获取结构体指针
macro_rules! container_of {
    ($ptr:expr, $type:ty, $field:ident) => {{
        let ptr = $ptr as *const u8;
        let offset = core::mem::offset_of!($type, $field);
        unsafe { (ptr.sub(offset)) as *mut $type }
    }};
}

static INVALID_MM_STATE_LOGGED: AtomicBool = AtomicBool::new(false);

#[inline]
fn mm_state_ready() -> bool {
    let base = vmemmap_base_ptr() as usize;
    let max = max_pfn();
    if base != 0 && max != 0 {
        return true;
    }
    if !INVALID_MM_STATE_LOGGED.swap(true, Ordering::SeqCst) {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "[diag][buddy] mm state invalid: vmemmap_base={:#x} max_pfn={} vmemmap_sym={:#x} max_pfn_sym={:#x}",
                base,
                max,
                core::ptr::addr_of!(super::page::VMEMMAP_BASE) as usize,
                core::ptr::addr_of!(super::page::MAX_PFN) as usize,
            );
        }
    }
    false
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

// ============================================================================
// Buddy 分配器核心
// ============================================================================

/// 分配 2^order 个连续页帧
///
/// # Arguments
/// * `order` - 分配 2^order 个页
/// * `gfp` - 分配标志
///
/// # Returns
/// 成功返回第一个 Page 的引用，失败返回 None
pub fn alloc_pages(order: usize, gfp: GfpFlags) -> Option<&'static mut Page> {
    if !mm_state_ready() {
        return None;
    }
    if order >= MAX_ORDER {
        return None;
    }

    // 遍历合适的 Zone 列表
    let zone_list = gfp_to_zone_list(gfp);

    for &zone_type in zone_list {
        let mut zone = get_zone(zone_type);
        if !zone.initialized {
            continue;
        }

        if let Some(page) = zone_alloc_pages(&mut zone, order, gfp) {
            if !is_page_ptr_in_vmemmap(page as *const Page) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "[diag][buddy] invalid page pointer from zone_alloc_pages: zone={:?} order={} page_ptr={:#x}",
                        zone_type,
                        order,
                        page as *mut Page as usize,
                    );
                }
                continue;
            }
            return Some(page);
        }
    }

    None
}

/// 从指定 Zone 分配页帧
fn zone_alloc_pages(zone: &mut Zone, order: usize, gfp: GfpFlags) -> Option<&'static mut Page> {
    // 获取 Zone 锁，保护 free_area 链表操作
    // 使用 addr_of! 避免借用冲突（协议锁模式）
    let _guard = unsafe { (*core::ptr::addr_of!(zone.lock)).lock() };

    // 从请求的 order 开始向上查找
    let mut current_order = order;

    while current_order < MAX_ORDER {
        let area = &zone.free_area[current_order];

        if !area.is_empty() {
            // 找到可用块
            return Some(unsafe { expand_and_alloc(zone, current_order, order, gfp) });
        }

        current_order += 1;
    }

    None
}

/// 分裂大块并分配
///
/// # Safety
///
/// 调用者必须确保 zone.free_area[high_order] 非空
unsafe fn expand_and_alloc(
    zone: &mut Zone,
    high_order: usize,
    low_order: usize,
    gfp: GfpFlags,
) -> &'static mut Page {
    unsafe {
        // 从高 order 空闲链表取出一个块
        let area = &mut zone.free_area[high_order];
        let list_ptr = area.free_list.next;

        // 通过链表节点找到 Page
        // Page.lru 是 ListHead，通过偏移计算 Page 地址
        let page = container_of!(list_ptr, Page, lru);

        // 从 Buddy 系统移除
        zone.remove_from_buddy(&mut *page, high_order);

        // 分裂：从 high_order 分裂到 low_order
        let mut current_order = high_order;
        let pfn = page_to_pfn(&*page);

        while current_order > low_order {
            current_order -= 1;

            // 计算伙伴 PFN（后半部分）
            let buddy_pfn = pfn + pages_per_order(current_order);
            let buddy_page = pfn_to_page(buddy_pfn);

            // 将伙伴块加入空闲链表
            zone.add_to_buddy(&mut *buddy_page, current_order);
        }

        // 设置分配的页
        let page = &mut *page;
        page.set_count_one();
        page.set_order(low_order as u8);
        page.clear_flag(PageFlags::BUDDY);

        // 如果请求清零
        if gfp.test(GfpFlags::ZERO) {
            let addr = pfn * PAGE_SIZE;
            let size = pages_per_order(low_order) * PAGE_SIZE;
            // 通过直接映射访问物理内存来清零
            let virt = DIRECT_MAP_OFFSET + addr;
            core::ptr::write_bytes(virt as *mut u8, 0, size as usize);
        }

        // 如果是复合页，设置复合页标志
        if gfp.test(GfpFlags::COMP) && low_order > 0 {
            prep_compound_page(page, low_order);
        }

        page
    }
}

/// 释放 2^order 个连续页帧
///
/// # Safety
///
/// - page 必须是之前通过 alloc_pages 分配的
/// - order 必须与分配时相同
pub unsafe fn free_pages(page: &mut Page, order: usize) {
    unsafe {
        if order >= MAX_ORDER {
            return;
        }

        // 检测 double-free
        if page.refcount() == 0 {
            crate::warn!(
                "BUG: double-free detected for page PFN {}",
                page_to_pfn(page)
            );
            return;
        }

        // 减少引用计数
        if page.put() > 0 {
            return; // 还有其他引用
        }

        // 清除复合页标志
        if page.is_compound() {
            destroy_compound_page(page, order);
        }

        // order-0 优先回收到 PCP，降低 Zone 锁竞争
        if order == 0 && pcp_initialized() {
            pcp_free_page(page);
            return;
        }

        let mut pfn = page_to_pfn(page);
        let mut zone = match super::zone::pfn_to_zone(pfn) {
            Some(z) => z,
            None => return,
        };

        // 获取 Zone 锁，保护 free_area 链表操作
        let _guard = (*core::ptr::addr_of!(zone.lock)).lock();

        // 尝试合并伙伴
        let mut current_order = order;

        while current_order < MAX_ORDER - 1 {
            let buddy_pfn = get_buddy_pfn(pfn, current_order);

            // 检查伙伴是否在同一 Zone 且空闲
            if !zone.contains_pfn(buddy_pfn) {
                break;
            }

            let buddy_page = pfn_to_page(buddy_pfn);

            // 检查伙伴是否在 Buddy 系统中且 order 相同
            if !(*buddy_page).is_buddy() || (*buddy_page).order() != current_order as u8 {
                break;
            }

            // 从空闲链表移除伙伴
            zone.remove_from_buddy(&mut *buddy_page, current_order);

            // 合并：使用较小的 PFN
            pfn = pfn.min(buddy_pfn);
            current_order += 1;
        }

        // 将合并后的块加入空闲链表
        let page = pfn_to_page(pfn);
        zone.add_to_buddy(&mut *page, current_order);
    }
}

/// 分配单个页帧
#[inline]
pub fn alloc_page(gfp: GfpFlags) -> Option<&'static mut Page> {
    if !mm_state_ready() {
        return None;
    }
    if pcp_initialized() {
        if let Some(page) = pcp_alloc_page(gfp) {
            if !is_page_ptr_in_vmemmap(page as *const Page) {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "[diag][buddy] invalid page pointer from pcp_alloc_page: page_ptr={:#x}",
                        page as *mut Page as usize,
                    );
                }
                return alloc_pages(0, gfp);
            }

            page.set_count_one();

            // PCP 快路径同样需要满足 GFP 语义。
            page.clear_flag(PageFlags::BUDDY);
            page.set_order(0);

            if gfp.test(GfpFlags::ZERO) {
                let addr = page_to_pfn(page) * PAGE_SIZE;
                let virt = DIRECT_MAP_OFFSET + addr;
                unsafe {
                    core::ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE as usize);
                }
            }

            return Some(page);
        }
    }

    alloc_pages(0, gfp)
}

/// 释放单个页帧
#[inline]
pub unsafe fn free_page(page: &mut Page) {
    unsafe { free_pages(page, 0) }
}

// ============================================================================
// 复合页支持
// ============================================================================

/// 准备复合页
fn prep_compound_page(page: &mut Page, order: usize) {
    let nr_pages = pages_per_order(order);

    // 头页
    page.set_flag(PageFlags::HEAD | PageFlags::COMPOUND);

    // 尾页
    for i in 1..nr_pages {
        unsafe {
            let pfn = page_to_pfn(page) + i;
            let tail = pfn_to_page(pfn);
            (*tail).set_flag(PageFlags::TAIL | PageFlags::COMPOUND);
        }
    }
}

/// 销毁复合页
unsafe fn destroy_compound_page(page: &mut Page, order: usize) {
    unsafe {
        let nr_pages = pages_per_order(order);

        // 清除所有页的复合页标志
        for i in 0..nr_pages {
            let pfn = page_to_pfn(page) + i;
            let p = pfn_to_page(pfn);
            (*p).clear_flag(PageFlags::HEAD | PageFlags::TAIL | PageFlags::COMPOUND);
        }
    }
}

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 Buddy 系统
///
/// 将内存区域添加到 Buddy 系统
///
/// # Safety
///
/// - start_pfn 和 end_pfn 必须有效
/// - Page 数组必须已初始化
pub unsafe fn init_zone_buddy(zone: &mut Zone, start_pfn: u64, end_pfn: u64) {
    unsafe {
        // 将每个页帧添加到 Buddy 系统
        // 从最大的 order 开始，尽可能使用大块

        let mut pfn = start_pfn;

        while pfn < end_pfn {
            // 找到当前 PFN 能使用的最大 order
            let mut order = MAX_ORDER - 1;

            while order > 0 {
                let block_pages = pages_per_order(order);

                // 检查：1) 对齐到 order 边界 2) 不超出范围
                if (pfn % block_pages) == 0 && (pfn + block_pages) <= end_pfn {
                    break;
                }
                order -= 1;
            }

            let page = pfn_to_page(pfn);
            zone.add_to_buddy(&mut *page, order);

            pfn += pages_per_order(order);
        }
    }
}

// ============================================================================
// 调试和统计
// ============================================================================

/// 获取 Zone 的 Buddy 统计信息
pub fn zone_buddy_stats(zone: &Zone) -> [u64; MAX_ORDER] {
    let mut stats = [0u64; MAX_ORDER];
    for (i, area) in zone.free_area.iter().enumerate() {
        stats[i] = area.nr_free;
    }
    stats
}

/// 打印 Buddy 系统状态
pub fn print_buddy_info(zone: &Zone) {
    let stats = zone_buddy_stats(zone);
    // 打印每个 order 的空闲块数
    // 由调用者实现具体打印
    let _ = stats;
}
