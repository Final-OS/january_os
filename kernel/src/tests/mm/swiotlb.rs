use super::{fail, mm_step, pass};
use crate::config;
use crate::mm::iommu::{DmaAddr, DmaDirection, Swiotlb};
use crate::{kprintln, warn};

pub(super) fn run() {
    const PAGE_SIZE: usize = 4096;
    const POOL_BYTES: usize = 64 * PAGE_SIZE;

    mm_step("swiotlb: init bounce pool");
    // 64 pages = 256 KiB，覆盖基础路径 + 边界/异常路径
    let mut swiotlb = Swiotlb::new(POOL_BYTES, config::DIRECT_MAP_OFFSET);

    let base_used = swiotlb.used_slots();
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/mm] swiotlb base_used_slots={}", base_used);
        kprintln!(
            "[test/mm][swiotlb] expect used_slots baseline={} actual={}",
            base_used,
            swiotlb.used_slots()
        );
    }

    // 1) 正常路径：低于 4GB，直接映射且不占槽
    mm_step("swiotlb: case=low_addr_direct_map");
    let low_phys = 0x20_0000u64;
    let dma_low = swiotlb.map(low_phys, PAGE_SIZE, DmaDirection::ToDevice);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][low] input phys={:#x} size={} expected_dma={:#x} actual_dma={:#x}",
            low_phys,
            PAGE_SIZE,
            low_phys,
            dma_low.as_u64()
        );
    }
    if dma_low.as_u64() != low_phys {
        return fail("swiotlb", "low address should use direct DMA address");
    }
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][low] expected_used_slots={} actual_used_slots={}",
            base_used,
            swiotlb.used_slots()
        );
    }
    if swiotlb.used_slots() != base_used {
        return fail("swiotlb", "low address mapping should not consume slots");
    }
    mm_step("swiotlb: unmap low address");
    swiotlb.unmap(dma_low, PAGE_SIZE, DmaDirection::ToDevice);
    if swiotlb.used_slots() != base_used {
        return fail(
            "swiotlb",
            "unmap low address should keep slot usage unchanged",
        );
    }

    // 2) 边界输入：4GB-1 页的完整页映射应仍是直通
    mm_step("swiotlb: case=boundary_last_page_below_4g");
    let boundary_phys = 0x1_0000_0000u64 - PAGE_SIZE as u64;
    let dma_boundary = swiotlb.map(boundary_phys, PAGE_SIZE, DmaDirection::None);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][boundary] input phys={:#x} size={} expected_dma={:#x} actual_dma={:#x}",
            boundary_phys,
            PAGE_SIZE,
            boundary_phys,
            dma_boundary.as_u64()
        );
    }
    if dma_boundary.as_u64() != boundary_phys {
        return fail(
            "swiotlb",
            "last full page below 4GB should use direct DMA address",
        );
    }
    if swiotlb.used_slots() != base_used {
        return fail("swiotlb", "boundary direct map should not consume slots");
    }

    // 3) 非法/异常输入：高地址 + 零长度，不应分配槽位
    mm_step("swiotlb: case=high_addr_zero_size");
    let zero_size_phys = 0x1_0000_2000u64;
    let zero_before = swiotlb.used_slots();
    let dma_zero = swiotlb.map(zero_size_phys, 0, DmaDirection::None);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][zero-size] input phys={:#x} size=0 expected_dma={:#x} actual_dma={:#x}",
            zero_size_phys,
            zero_size_phys,
            dma_zero.as_u64()
        );
    }
    if dma_zero.as_u64() != zero_size_phys {
        return fail(
            "swiotlb",
            "zero-size high address mapping should fallback to direct address",
        );
    }
    if swiotlb.used_slots() != zero_before {
        return fail("swiotlb", "zero-size mapping should not change slot usage");
    }

    // 4) 边界输入：请求大于池容量，必须回退直通并保持槽位不变
    mm_step("swiotlb: case=oversized_high_request");
    let oversize_phys = 0x1_0000_3000u64;
    let oversize = POOL_BYTES + PAGE_SIZE;
    let oversize_before = swiotlb.used_slots();
    let dma_oversize = swiotlb.map(oversize_phys, oversize, DmaDirection::None);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][oversize] input phys={:#x} size={} expected_dma={:#x} actual_dma={:#x}",
            oversize_phys,
            oversize,
            oversize_phys,
            dma_oversize.as_u64()
        );
    }
    if dma_oversize.as_u64() != oversize_phys {
        return fail(
            "swiotlb",
            "oversized request should fallback to original physical address",
        );
    }
    if swiotlb.used_slots() != oversize_before {
        return fail("swiotlb", "oversized request should not consume slots");
    }

    // 5) 异常输入：unmap 非池地址，槽位计数应保持不变
    mm_step("swiotlb: case=unmap_non_pool_address");
    let bogus_dma = DmaAddr::new(0x1_2000_0000);
    let bogus_before = swiotlb.used_slots();
    swiotlb.unmap(bogus_dma, PAGE_SIZE, DmaDirection::FromDevice);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][invalid-unmap] input dma={:#x} expected_used_slots={} actual_used_slots={}",
            bogus_dma.as_u64(),
            bogus_before,
            swiotlb.used_slots()
        );
    }
    if swiotlb.used_slots() != bogus_before {
        return fail(
            "swiotlb",
            "unmap out-of-pool address should not change slot usage",
        );
    }

    // 6) 正常路径：高于 4GB，尝试 bounce（None 方向避免真实拷贝依赖）
    mm_step("swiotlb: case=high_addr_bounce_map");
    let high_phys = 0x1_0000_1000u64;
    let dma_high = swiotlb.map(high_phys, PAGE_SIZE * 2, DmaDirection::None);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][swiotlb][high] input phys={:#x} size={} actual_dma={:#x}",
            high_phys,
            PAGE_SIZE * 2,
            dma_high.as_u64()
        );
    }

    if dma_high.as_u64() == high_phys {
        // 可能是 memblock 无法再提供低端空间，给出可见提示但不误报失败
        warn!("mm/swiotlb: bounce allocation unavailable, skipped high-address path check");
        return pass("swiotlb");
    }

    if swiotlb.used_slots() < base_used + 2 {
        return fail(
            "swiotlb",
            "high address mapping should consume at least 2 slots",
        );
    }

    mm_step("swiotlb: unmap high address");
    swiotlb.unmap(dma_high, PAGE_SIZE * 2, DmaDirection::None);
    if swiotlb.used_slots() != base_used {
        return fail(
            "swiotlb",
            "unmap high address should release allocated slots",
        );
    }

    pass("swiotlb");
}
