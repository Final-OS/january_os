use super::{fail, mm_step, pass};
use crate::mm;
use crate::warn;

pub(super) fn run() {
    mm_step("page_counter_guard: case=refcount_underflow_rejected");
    let base = mm::page_guard_stats();

    let page = match mm::alloc_page(mm::GFP_KERNEL) {
        Some(p) => p,
        None => {
            warn!("mm/page_counter_guard: alloc_page failed, skip checks");
            return pass("page_counter_guard");
        }
    };

    unsafe {
        mm::free_page(page);
        mm::free_page(page);
    }

    let after_ref = mm::page_guard_stats();
    if after_ref.ref_underflow_rejects < base.ref_underflow_rejects + 1 {
        return fail(
            "page_counter_guard",
            "refcount underflow reject counter did not increase",
        );
    }

    mm_step("page_counter_guard: case=mapcount_underflow_rejected");
    let page2 = match mm::alloc_page(mm::GFP_KERNEL) {
        Some(p) => p,
        None => return fail("page_counter_guard", "second alloc_page failed"),
    };

    let mapcount_base = mm::page_guard_stats();
    if page2.try_dec_mapcount().is_ok() {
        unsafe { mm::free_page(page2) };
        return fail(
            "page_counter_guard",
            "try_dec_mapcount should reject mapcount underflow from -1",
        );
    }
    let mapcount_after = mm::page_guard_stats();
    if mapcount_after.mapcount_underflow_rejects < mapcount_base.mapcount_underflow_rejects + 1 {
        unsafe { mm::free_page(page2) };
        return fail(
            "page_counter_guard",
            "mapcount underflow reject counter did not increase",
        );
    }

    unsafe { mm::free_page(page2) };
    pass("page_counter_guard");
}
