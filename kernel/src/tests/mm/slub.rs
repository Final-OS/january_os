use super::{fail, mm_step, pass};
use crate::config;
use crate::kprintln;

pub(super) fn run() {
    use crate::mm::GFP_KERNEL;
    use crate::mm::page::page::pfn_to_page;
    use crate::mm::slub::{kfree, kmalloc};
    use core::ptr;

    // 1) 异常输入：kfree(null) 必须是 no-op
    mm_step("slub: case=kfree_null_noop");
    unsafe {
        kfree(ptr::null_mut());
    }
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/mm][slub][null-free] expected=no panic actual=ok");
    }

    // 2) 非法输入：kfree(非 direct-map 指针) 必须是 no-op
    mm_step("slub: case=kfree_non_direct_map_noop");
    unsafe {
        kfree(0x12345usize as *mut u8);
    }
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/mm][slub][non-direct-map-free] expected=no panic actual=ok");
    }

    // 3) 边界输入：kmalloc(0) 允许返回 NULL 或最小可用对象（实现相关）
    mm_step("slub: case=kmalloc_zero_size");
    let zero = kmalloc(0, GFP_KERNEL);
    if zero.is_null() {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][slub][zero] input size=0 expected=(null or valid ptr) actual_ptr=0x0"
            );
        }
    } else {
        if crate::config::DEBUG_VERBOSE {
            kprintln!(
                "[test/mm][slub][zero] input size=0 expected=(null or valid ptr) actual_ptr={:#x}",
                zero as usize
            );
        }
        if (zero as u64) < config::DIRECT_MAP_OFFSET {
            unsafe { kfree(zero) };
            return fail(
                "slub",
                "kmalloc(0) returned non-null ptr outside direct-map range",
            );
        }
    }

    // 4) 正常路径：小对象走 SLUB cache
    mm_step("slub: case=small_alloc_64");
    let small = kmalloc(64, GFP_KERNEL);
    if small.is_null() {
        unsafe { kfree(zero) };
        return fail("slub", "kmalloc(64) returned null");
    }
    if (small as u64) < config::DIRECT_MAP_OFFSET {
        unsafe {
            kfree(small);
            kfree(zero);
        }
        return fail(
            "slub",
            "small allocation is not in direct-map virtual range",
        );
    }

    // 5) 同类对象并发分配：两个 64B 指针不应相同
    mm_step("slub: case=small_alloc_uniqueness");
    let small2 = kmalloc(64, GFP_KERNEL);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][slub][small-uniq] ptr1={:#x} ptr2={:#x}",
            small as usize,
            small2 as usize
        );
    }
    if small2.is_null() {
        unsafe {
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "second kmalloc(64) returned null");
    }
    if small2 == small {
        unsafe {
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "two live kmalloc(64) allocations should not alias");
    }

    // 6) 阈值边界：8192 与 8193
    mm_step("slub: case=threshold_8192");
    let boundary_in = kmalloc(8192, GFP_KERNEL);
    if boundary_in.is_null() {
        unsafe {
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "kmalloc(8192) returned null");
    }
    if (boundary_in as u64) < config::DIRECT_MAP_OFFSET {
        unsafe {
            kfree(boundary_in);
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "kmalloc(8192) is not in direct-map virtual range");
    }

    mm_step("slub: case=threshold_8193");
    let boundary_out = kmalloc(8193, GFP_KERNEL);
    if boundary_out.is_null() {
        unsafe {
            kfree(boundary_in);
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "kmalloc(8193) returned null");
    }
    if (boundary_out as u64) < config::DIRECT_MAP_OFFSET {
        unsafe {
            kfree(boundary_out);
            kfree(boundary_in);
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "kmalloc(8193) is not in direct-map virtual range");
    }

    // 7) 大对象：走 buddy 路径（size > 8192）
    mm_step("slub: case=large_alloc_16k");
    let large = kmalloc(16 * 1024, GFP_KERNEL);
    if large.is_null() {
        unsafe {
            kfree(boundary_out);
            kfree(boundary_in);
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail("slub", "kmalloc(16K) returned null");
    }

    if (large as u64) < config::DIRECT_MAP_OFFSET {
        unsafe {
            kfree(large);
            kfree(boundary_out);
            kfree(boundary_in);
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail(
            "slub",
            "large allocation is not in direct-map virtual range",
        );
    }

    // 8) 非法输入：对大块分配传入非页对齐地址给 kfree，必须是 no-op
    mm_step("slub: case=large_alloc_unaligned_free_noop");
    let large_phys = (large as u64).saturating_sub(config::DIRECT_MAP_OFFSET);
    let large_pfn = large_phys / crate::mm::PAGE_SIZE;
    let page_before = unsafe { (&*pfn_to_page(large_pfn)).refcount() };
    unsafe {
        kfree(large.add(16));
    }
    let page_after = unsafe { (&*pfn_to_page(large_pfn)).refcount() };
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][slub][unaligned-free] pfn={} ref_before={} ref_after={}",
            large_pfn,
            page_before,
            page_after
        );
    }
    if page_after != page_before {
        unsafe {
            kfree(large);
            kfree(boundary_out);
            kfree(boundary_in);
            kfree(small2);
            kfree(small);
            kfree(zero);
        }
        return fail(
            "slub",
            "unaligned kfree for large allocation unexpectedly changed page refcount",
        );
    }

    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/mm][slub] ptrs zero={:#x} small={:#x} small2={:#x} 8192={:#x} 8193={:#x} 16k={:#x}",
            zero as usize,
            small as usize,
            small2 as usize,
            boundary_in as usize,
            boundary_out as usize,
            large as usize
        );
    }

    unsafe {
        mm_step("slub: cleanup all allocations");
        kfree(large);
        kfree(boundary_out);
        kfree(boundary_in);
        kfree(small2);
        kfree(small);
        kfree(zero);
    }

    pass("slub");
}
