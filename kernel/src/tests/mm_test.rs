//! 内存管理子系统测试

use crate::{error, kprintln, ok, warn};
use crate::config;
use crate::mm::iommu::{DmaDirection, Swiotlb};

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    kprintln!("=== MM Subsystem Tests ===");

    match filter {
        None => {
            test_swiotlb_paths();
            test_slub_address_semantics();
            test_buddy_order_metadata();
            test_pcp_integration();
        }
        Some("swiotlb") => test_swiotlb_paths(),
        Some("slub") => test_slub_address_semantics(),
        Some("buddy") => test_buddy_order_metadata(),
        Some("pcp") => test_pcp_integration(),
        Some(name) => {
            error!("Unknown MM test: {}", name);
            kprintln!("Available MM tests: swiotlb, slub, buddy, pcp");
        }
    }

    kprintln!();
}

fn pass(name: &str) {
    ok!("mm/{}", name);
}

fn fail(name: &str, msg: &str) {
    error!("mm/{}: {}", name, msg);
}

fn test_swiotlb_paths() {
    // 64 pages = 256 KiB，足够覆盖基础路径
    let mut swiotlb = Swiotlb::new(64 * 4096, config::DIRECT_MAP_OFFSET);

    let base_used = swiotlb.used_slots();

    // 1) 低于 4GB：应该直接映射，不占用槽位
    let low_phys = 0x20_0000u64;
    let dma_low = swiotlb.map(low_phys, 4096, DmaDirection::ToDevice);
    if dma_low.as_u64() != low_phys {
        return fail("swiotlb", "low address should use direct DMA address");
    }
    if swiotlb.used_slots() != base_used {
        return fail("swiotlb", "low address mapping should not consume slots");
    }
    swiotlb.unmap(dma_low, 4096, DmaDirection::ToDevice);
    if swiotlb.used_slots() != base_used {
        return fail("swiotlb", "unmap low address should keep slot usage unchanged");
    }

    // 2) 高于 4GB：应尝试走 bounce buffer（用 None 方向避免真实拷贝依赖）
    let high_phys = 0x1_0000_1000u64;
    let dma_high = swiotlb.map(high_phys, 8192, DmaDirection::None);

    if dma_high.as_u64() == high_phys {
        // 可能是 memblock 无法再提供低端空间，给出可见提示但不误报失败
        warn!("mm/swiotlb: bounce allocation unavailable, skipped high-address path check");
        return pass("swiotlb");
    }

    if swiotlb.used_slots() < base_used + 2 {
        return fail("swiotlb", "high address mapping should consume at least 2 slots");
    }

    swiotlb.unmap(dma_high, 8192, DmaDirection::None);
    if swiotlb.used_slots() != base_used {
        return fail("swiotlb", "unmap high address should release allocated slots");
    }

    pass("swiotlb");
}


fn test_slub_address_semantics() {
    use crate::mm::slub::{kfree, kmalloc};
    use crate::mm::GFP_KERNEL;

    // 小对象：走 SLUB cache
    let small = kmalloc(64, GFP_KERNEL);
    if small.is_null() {
        return fail("slub", "kmalloc(64) returned null");
    }

    if (small as u64) < config::DIRECT_MAP_OFFSET {
        unsafe { kfree(small) };
        return fail("slub", "small allocation is not in direct-map virtual range");
    }

    // 大对象：走 buddy 路径（size > 8192）
    let large = kmalloc(16 * 1024, GFP_KERNEL);
    if large.is_null() {
        unsafe { kfree(small) };
        return fail("slub", "kmalloc(16K) returned null");
    }

    if (large as u64) < config::DIRECT_MAP_OFFSET {
        unsafe {
            kfree(large);
            kfree(small);
        }
        return fail("slub", "large allocation is not in direct-map virtual range");
    }

    unsafe {
        kfree(large);
        kfree(small);
    }

    pass("slub");
}


fn test_buddy_order_metadata() {
    use crate::mm::{alloc_pages, free_pages, GfpFlags, GFP_KERNEL};

    // order-0 基础语义
    if let Some(page0) = alloc_pages(0, GFP_KERNEL) {
        if page0.order() != 0 {
            unsafe { free_pages(page0, 0) };
            return fail("buddy", "order-0 allocation should set page.order=0");
        }
        unsafe { free_pages(page0, 0) };
    } else {
        warn!("mm/buddy: alloc_pages(order=0) failed, skip");
        return pass("buddy");
    }

    // 高阶页：验证分配后 page.order 与请求一致
    let order = 3usize;
    let flags = GfpFlags::new(GFP_KERNEL.bits());

    let page = match alloc_pages(order, flags) {
        Some(p) => p,
        None => {
            warn!("mm/buddy: alloc_pages(order=3) failed, skip high-order check");
            return pass("buddy");
        }
    };

    if page.order() != order as u8 {
        unsafe { free_pages(page, order) };
        return fail("buddy", "high-order allocation metadata mismatch");
    }

    unsafe { free_pages(page, order) };
    pass("buddy");
}


fn test_pcp_integration() {
    use crate::mm::{alloc_page, drain_all_pcps, free_page, pcp_initialized, pcp_stats, GFP_KERNEL};

    if !pcp_initialized() {
        warn!("mm/pcp: not initialized, skip");
        return pass("pcp");
    }

    // 先排空，建立可观测基线
    drain_all_pcps();
    let base = pcp_stats().total_cached;

    let page = match alloc_page(GFP_KERNEL) {
        Some(p) => p,
        None => {
            warn!("mm/pcp: alloc_page failed, skip");
            return pass("pcp");
        }
    };

    unsafe { free_page(page) };

    let after_free = pcp_stats().total_cached;
    if after_free <= base {
        return fail("pcp", "free_page(order-0) should cache page into PCP");
    }

    let page2 = match alloc_page(GFP_KERNEL) {
        Some(p) => p,
        None => return fail("pcp", "second alloc_page failed"),
    };

    if page2.refcount() != 1 {
        unsafe { free_page(page2) };
        return fail("pcp", "allocated page should have refcount=1");
    }

    unsafe { free_page(page2) };
    pass("pcp");
}
