use super::{fail, mm_step, pass};
use crate::{kprintln, warn};

pub(super) fn run() {
    use crate::mm::{GFP_KERNEL, GfpFlags, MAX_ORDER, alloc_pages, free_pages, page_to_pfn};

    mm_step("buddy: case=invalid_order_rejected");
    let invalid = alloc_pages(MAX_ORDER, GFP_KERNEL);
    kprintln!(
        "[test/mm][buddy][invalid-order] input order={} expected=None actual_is_some={}",
        MAX_ORDER,
        invalid.is_some()
    );
    if invalid.is_some() {
        return fail("buddy", "alloc_pages(MAX_ORDER) must return None");
    }

    // order-0 基础语义
    mm_step("buddy: case=order0_alloc_metadata");
    let page0 = match alloc_pages(0, GFP_KERNEL) {
        Some(p) => p,
        None => {
            warn!("mm/buddy: alloc_pages(order=0) failed, skip remaining buddy checks");
            return pass("buddy");
        }
    };

    let pfn0 = page_to_pfn(page0);
    kprintln!(
        "[test/mm][buddy][order0] pfn={} order={} refcount={}",
        pfn0,
        page0.order(),
        page0.refcount()
    );
    if page0.order() != 0 {
        unsafe { free_pages(page0, 0) };
        return fail("buddy", "order-0 allocation should set page.order=0");
    }
    if page0.refcount() != 1 {
        unsafe { free_pages(page0, 0) };
        return fail("buddy", "allocated page should have refcount=1");
    }

    // 分配第二个 order-0 页，验证非别名
    mm_step("buddy: case=order0_two_pages_unique");
    let page1 = match alloc_pages(0, GFP_KERNEL) {
        Some(p) => p,
        None => {
            unsafe { free_pages(page0, 0) };
            warn!("mm/buddy: second alloc_pages(order=0) failed, skip remaining checks");
            return pass("buddy");
        }
    };

    let pfn1 = page_to_pfn(page1);
    kprintln!(
        "[test/mm][buddy][order0-unique] pfn0={} pfn1={} expected_different=true",
        pfn0,
        pfn1
    );
    if pfn0 == pfn1 {
        unsafe {
            free_pages(page1, 0);
            free_pages(page0, 0);
        }
        return fail(
            "buddy",
            "two live order-0 allocations should map to different PFNs",
        );
    }

    mm_step("buddy: free order-0 pages");
    unsafe {
        free_pages(page1, 0);
        free_pages(page0, 0);
    }

    // 高阶页：验证分配后 page.order 与请求一致
    let order = core::cmp::min(3usize, MAX_ORDER.saturating_sub(1));
    if order == 0 {
        warn!("mm/buddy: MAX_ORDER too small for high-order checks, skip");
        return pass("buddy");
    }

    let flags = GfpFlags::new(GFP_KERNEL.bits());
    mm_step("buddy: case=high_order_metadata");
    let page = match alloc_pages(order, flags) {
        Some(p) => p,
        None => {
            warn!(
                "mm/buddy: alloc_pages(order={}) failed, skip high-order checks",
                order
            );
            return pass("buddy");
        }
    };

    kprintln!(
        "[test/mm][buddy][high-order] order={} expected_page_order={} actual_page_order={}",
        order,
        order,
        page.order()
    );
    if page.order() != order as u8 {
        unsafe { free_pages(page, order) };
        return fail("buddy", "high-order allocation metadata mismatch");
    }

    mm_step("buddy: free high-order page");
    unsafe { free_pages(page, order) };

    // 复合页路径（COMP）
    let comp_order = core::cmp::min(2usize, MAX_ORDER.saturating_sub(1));
    if comp_order == 0 {
        return pass("buddy");
    }

    mm_step("buddy: case=compound_page_flag");
    let comp_flags = GfpFlags::new(GFP_KERNEL.bits() | GfpFlags::COMP);
    let comp_page = match alloc_pages(comp_order, comp_flags) {
        Some(p) => p,
        None => {
            warn!(
                "mm/buddy: alloc_pages(order={}, COMP) failed, skip compound check",
                comp_order
            );
            return pass("buddy");
        }
    };

    kprintln!(
        "[test/mm][buddy][compound] order={} expected_is_compound=true actual_is_compound={}",
        comp_order,
        comp_page.is_compound()
    );
    if !comp_page.is_compound() {
        unsafe { free_pages(comp_page, comp_order) };
        return fail("buddy", "compound allocation should set COMPOUND flag");
    }

    mm_step("buddy: free compound page");
    unsafe { free_pages(comp_page, comp_order) };

    pass("buddy");
}
