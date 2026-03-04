use super::{fail, mm_step, pass};

use core::alloc::Layout;
use core::ptr::null_mut;

const FILL_CHUNK: usize = 512 * 1024;
const FORCE_GROW_ALLOC: usize = 2 * 1024 * 1024;
const EXPECT_FAIL_ALLOC: usize = 8 * 1024 * 1024;
const RECOVERY_ALLOC: usize = 64 * 1024;
const MAX_ALLOCS: usize = 96;

pub fn run() {
    let init = crate::mm::heap::heap_stats();
    mm_step("heap: case=init_stats");
    if !init.initialized || init.total_size < crate::mm::PAGE_SIZE as usize {
        return fail("heap", "heap is not initialized or total size is invalid");
    }

    let mut ptrs = [null_mut(); MAX_ALLOCS];
    let mut sizes = [0usize; MAX_ALLOCS];
    let mut count = 0usize;

    mm_step("heap: case=auto_grow_when_free_insufficient");
    let mut shrink_guard = 0usize;
    while crate::mm::heap::heap_stats().free_size >= FORCE_GROW_ALLOC && count < MAX_ALLOCS - 1 {
        let layout = Layout::from_size_align(FILL_CHUNK, 64).expect("valid fill layout");
        let ptr = unsafe { crate::mm::heap::heap_alloc_raw(layout) };
        if ptr.is_null() {
            return fail("heap", "failed while preparing low-free condition for growth check");
        }
        ptrs[count] = ptr;
        sizes[count] = FILL_CHUNK;
        count += 1;
        shrink_guard += 1;
        if shrink_guard > MAX_ALLOCS {
            return fail("heap", "unexpected heap shrink loop overflow");
        }
    }

    let before = crate::mm::heap::heap_stats();
    let grow_layout = Layout::from_size_align(FORCE_GROW_ALLOC, 64).expect("valid grow layout");
    let grow_ptr = unsafe { crate::mm::heap::heap_alloc_raw(grow_layout) };
    if grow_ptr.is_null() {
        return fail("heap", "growth allocation failed");
    }
    ptrs[count] = grow_ptr;
    sizes[count] = FORCE_GROW_ALLOC;
    count += 1;

    let after = crate::mm::heap::heap_stats();
    if after.segments <= before.segments || after.total_size <= before.total_size {
        return fail("heap", "heap did not grow after free space became insufficient");
    }

    mm_step("heap: case=oversized_request_returns_null");
    let fail_layout = Layout::from_size_align(EXPECT_FAIL_ALLOC, 64).expect("valid fail layout");
    let fail_ptr = unsafe { crate::mm::heap::heap_alloc_raw(fail_layout) };
    if !fail_ptr.is_null() {
        unsafe {
            crate::mm::heap::heap_dealloc_raw(fail_ptr, fail_layout);
        }
        return fail("heap", "oversized allocation should fail with null");
    }

    mm_step("heap: case=recover_after_failed_alloc");
    let recovery_layout =
        Layout::from_size_align(RECOVERY_ALLOC, 64).expect("valid recovery layout");
    let recovery_ptr = unsafe { crate::mm::heap::heap_alloc_raw(recovery_layout) };
    if recovery_ptr.is_null() {
        return fail("heap", "heap cannot allocate small block after failed oversized request");
    }
    unsafe {
        crate::mm::heap::heap_dealloc_raw(recovery_ptr, recovery_layout);
    }

    mm_step("heap: cleanup allocations");
    while count > 0 {
        count -= 1;
        let layout = Layout::from_size_align(sizes[count], 64).expect("valid cleanup layout");
        unsafe {
            crate::mm::heap::heap_dealloc_raw(ptrs[count], layout);
        }
    }

    let final_stats = crate::mm::heap::heap_stats();
    if final_stats.live_allocations != 0 || final_stats.used_size != 0 {
        return fail("heap", "heap did not return to zero-live-allocation state after cleanup");
    }

    pass("heap");
}
