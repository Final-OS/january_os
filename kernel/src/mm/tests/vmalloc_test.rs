use crate::kprintln;
use crate::mm::vmalloc::{vmalloc, vfree, vmalloc_stats, vmalloc_dump_info};

/// 测试 vmalloc
pub fn test_vmalloc() {
    kprintln!("Testing vmalloc...");
    
    let start_stats = vmalloc_stats();
    kprintln!("Initial stats: areas={}, vm={}, phys={}", 
        start_stats.nr_areas, start_stats.total_vm, start_stats.total_phys);
    
    // 1. 分配测试
    let size = 8192; // 2 pages
    let ptr = vmalloc(size);
    
    if ptr.is_null() {
        kprintln!("vmalloc failed!");
        return;
    }
    
    kprintln!("vmalloc({}) returned {:?}", size, ptr);
    
    // 写入测试
    unsafe {
        *ptr = 0xAA;
        *ptr.add(4096) = 0xBB;
    }
    
    // 检查统计
    let mid_stats = vmalloc_stats();
    kprintln!("Mid stats: areas={}, vm={}, phys={}", 
        mid_stats.nr_areas, mid_stats.total_vm, mid_stats.total_phys);
        
    if mid_stats.nr_areas != start_stats.nr_areas + 1 {
        kprintln!("FAIL: Area count mismatch");
    }
    
    // 打印泄漏信息 (应该显示当前分配)
    vmalloc_dump_info();
    
    // 2. 释放测试
    vfree(ptr);
    
    let end_stats = vmalloc_stats();
    kprintln!("End stats: areas={}, vm={}, phys={}", 
        end_stats.nr_areas, end_stats.total_vm, end_stats.total_phys);
        
    if end_stats.nr_areas != start_stats.nr_areas {
        kprintln!("FAIL: Memory leak detected!");
    } else {
        kprintln!("vmalloc test passed.");
    }
}
