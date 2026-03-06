use super::{fail, mm_step, pass};
use crate::config;
use crate::{kprintln, warn};

fn stats_sum(per_zone: &[u64]) -> u64 {
    per_zone.iter().copied().sum()
}

pub(super) fn run() {
    use crate::mm::{
        alloc_page, drain_all_pcps, free_page, page_to_pfn, pcp_initialized, pcp_stats, GFP_KERNEL,
        GFP_KERNEL_ZERO,
    };

    mm_step("pcp: case=init_state_check");
    if !pcp_initialized() {
        let stats = pcp_stats();
        let sum = stats_sum(&stats.per_zone);
        kprintln!(
            "[test/mm][pcp][init] expected total=0 actual_total={} per_zone_sum={}",
            stats.total_cached,
            sum
        );
        if stats.total_cached != 0 || sum != 0 {
            return fail("pcp", "pcp not initialized but stats are non-zero");
        }
        warn!("mm/pcp: not initialized, skip runtime PCP checks");
        return pass("pcp");
    }

    // 先排空，建立可观测基线
    mm_step("pcp: case=drain_idempotent_baseline");
    drain_all_pcps();
    let after_first_drain = pcp_stats();
    drain_all_pcps();
    let after_second_drain = pcp_stats();

    let first_sum = stats_sum(&after_first_drain.per_zone);
    let second_sum = stats_sum(&after_second_drain.per_zone);
    kprintln!(
        "[test/mm][pcp][drain] first total={} sum={} second total={} sum={}",
        after_first_drain.total_cached,
        first_sum,
        after_second_drain.total_cached,
        second_sum
    );
    if first_sum != after_first_drain.total_cached {
        return fail("pcp", "pcp stats inconsistent after first drain");
    }
    if second_sum != after_second_drain.total_cached {
        return fail("pcp", "pcp stats inconsistent after second drain");
    }
    if after_first_drain.total_cached != after_second_drain.total_cached {
        return fail(
            "pcp",
            "drain_all_pcps should be idempotent on empty cache baseline",
        );
    }

    let base = after_second_drain.total_cached;

    // 正常路径：free 后应进入 PCP 缓存
    mm_step("pcp: case=free_caches_order0_page");
    let page = match alloc_page(GFP_KERNEL) {
        Some(p) => p,
        None => {
            warn!("mm/pcp: alloc_page failed, skip remaining checks");
            return pass("pcp");
        }
    };

    kprintln!(
        "[test/mm][pcp][alloc1] pfn={} refcount={} expected_refcount=1",
        page_to_pfn(page),
        page.refcount()
    );
    if page.refcount() != 1 {
        unsafe { free_page(page) };
        return fail("pcp", "allocated page should have refcount=1");
    }

    unsafe { free_page(page) };

    let after_free_stats = pcp_stats();
    let after_free = after_free_stats.total_cached;
    kprintln!(
        "[test/mm][pcp][cache] base_cached={} after_free_cached={}",
        base,
        after_free
    );
    if after_free <= base {
        return fail("pcp", "free_page(order-0) should cache page into PCP");
    }

    // 从 PCP 再取一页，确保分配路径正常
    mm_step("pcp: case=alloc_from_cached_pool");
    let page2 = match alloc_page(GFP_KERNEL) {
        Some(p) => p,
        None => return fail("pcp", "second alloc_page failed"),
    };
    let pfn2 = page_to_pfn(page2);
    kprintln!(
        "[test/mm][pcp][alloc2] pfn={} refcount={} expected_refcount=1",
        pfn2,
        page2.refcount()
    );
    if page2.refcount() != 1 {
        unsafe { free_page(page2) };
        return fail("pcp", "allocated cached page should have refcount=1");
    }

    unsafe { free_page(page2) };

    // 负向/边界：验证 GFP_ZERO 语义
    mm_step("pcp: case=gfp_zero_memory_cleared");
    let marker_page = match alloc_page(GFP_KERNEL) {
        Some(p) => p,
        None => {
            warn!("mm/pcp: marker alloc_page failed, skip ZERO check");
            return pass("pcp");
        }
    };

    let marker_pfn = page_to_pfn(marker_page);
    let marker_virt = config::DIRECT_MAP_OFFSET + marker_pfn * config::PAGE_SIZE;
    unsafe {
        core::ptr::write_bytes(marker_virt as *mut u8, 0xA5, 64);
        free_page(marker_page);
    }

    let zero_page = match alloc_page(GFP_KERNEL_ZERO) {
        Some(p) => p,
        None => return fail("pcp", "alloc_page(GFP_KERNEL_ZERO) failed"),
    };

    let zero_pfn = page_to_pfn(zero_page);
    let zero_virt = config::DIRECT_MAP_OFFSET + zero_pfn * config::PAGE_SIZE;
    let mut zero_ok = true;
    unsafe {
        for idx in 0..8usize {
            let val = *((zero_virt as *const u64).add(idx));
            if val != 0 {
                kprintln!(
                    "[test/mm][pcp][zero] non-zero word idx={} val={:#x} pfn={}",
                    idx,
                    val,
                    zero_pfn
                );
                zero_ok = false;
                break;
            }
        }
    }
    kprintln!(
        "[test/mm][pcp][zero] marker_pfn={} zero_pfn={} expected_zero=true actual_zero={}",
        marker_pfn,
        zero_pfn,
        zero_ok
    );
    unsafe { free_page(zero_page) };
    if !zero_ok {
        return fail(
            "pcp",
            "GFP_KERNEL_ZERO allocation should return zeroed memory",
        );
    }

    mm_step("pcp: case=stats_consistency_after_ops");
    let final_stats = pcp_stats();
    let final_sum = stats_sum(&final_stats.per_zone);
    kprintln!(
        "[test/mm][pcp][final] total_cached={} per_zone_sum={}",
        final_stats.total_cached,
        final_sum
    );
    if final_sum != final_stats.total_cached {
        return fail("pcp", "final PCP stats per_zone sum mismatch total");
    }

    pass("pcp");
}
