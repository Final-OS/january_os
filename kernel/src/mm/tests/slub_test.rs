use crate::kprintln;
use crate::mm::slub::{kmalloc, kfree, kzalloc};
use crate::mm::zone::GfpFlags;
use core::ptr;

pub fn test_slub() {
    kprintln!("Testing SLUB Allocator...");

    // 1. Small allocation (32 bytes)
    kprintln!("  [1] Small allocation (32B)");
    let ptr1 = kmalloc(32, GfpFlags::new(GfpFlags::KERNEL));
    if ptr1.is_null() {
        kprintln!("    FAIL: kmalloc(32) failed");
        return;
    }
    unsafe {
        ptr::write_bytes(ptr1, 0xAA, 32);
        if *ptr1 != 0xAA {
             kprintln!("    FAIL: Memory write failed");
        }
        kfree(ptr1);
    }

    // 2. Large allocation (2048 bytes)
    kprintln!("  [2] Large allocation (2048B)");
    let ptr2 = kmalloc(2048, GfpFlags::new(GfpFlags::KERNEL));
    if ptr2.is_null() {
        kprintln!("    FAIL: kmalloc(2048) failed");
        return;
    }
    unsafe { kfree(ptr2) };

    // 3. Zero allocation (kzalloc)
    kprintln!("  [3] Zero allocation (kzalloc 64B)");
    let ptr3 = kzalloc(64, GfpFlags::new(GfpFlags::KERNEL));
    if ptr3.is_null() {
        kprintln!("    FAIL: kzalloc(64) failed");
        return;
    }
    unsafe {
        let slice = core::slice::from_raw_parts(ptr3, 64);
        let mut is_zero = true;
        for &b in slice {
            if b != 0 {
                is_zero = false;
                break;
            }
        }
        if !is_zero {
            kprintln!("    FAIL: Memory not zeroed");
        }
        kfree(ptr3);
    }

    kprintln!("SLUB tests passed.");
}
