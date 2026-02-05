use crate::kprintln;
use crate::mm::vma::{Mm, Vma, VmFlags};

/// 测试 VMA
pub fn test_vma() {
    kprintln!("Testing VMA...");
    
    // 创建一个临时的 Mm
    // 注意：Mm::init 需要 pgd，这里我们只是测试 VMA 链表操作，不需要真实的页表
    let mut mm = Mm::uninit();
    mm.init(0);
    
    // 1. 创建 VMA
    let mut vma1 = Vma::uninit();
    vma1.init(0x1000, 0x2000, VmFlags::new(VmFlags::READ | VmFlags::WRITE));
    
    unsafe {
        mm.insert_vma(&mut vma1);
    }
    
    if mm.vma_count != 1 {
        kprintln!("FAIL: vma_count mismatch after insert");
    }
    
    // 2. 查找测试
    if let Some(v) = mm.find_vma(0x1000) {
        if v.vm_start != 0x1000 {
            kprintln!("FAIL: Found wrong VMA");
        }
    } else {
        kprintln!("FAIL: Failed to find VMA");
    }
    
    if mm.find_vma(0x3000).is_some() {
        kprintln!("FAIL: Found non-existent VMA");
    }
    
    // 3. 移除测试
    unsafe {
        mm.remove_vma(&mut vma1);
    }
    
    if mm.vma_count != 0 {
        kprintln!("FAIL: vma_count mismatch after remove");
    }
    
    kprintln!("VMA test passed.");
}
