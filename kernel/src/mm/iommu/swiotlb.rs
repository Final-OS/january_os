// ============================================================================
// SWIOTLB (Software I/O TLB) 实现
//
// 当没有硬件 IOMMU 时使用的软件弹跳缓冲区
// 用于 32 位设备访问高地址内存
// ============================================================================

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use super::{DmaAddr, PAGE_SIZE};

/// SWIOTLB 最大槽位数 (64MB / 4KB = 16384)
const MAX_SLOTS: usize = 16384;

/// 槽位状态
#[derive(Clone, Copy, PartialEq)]
enum SlotState {
    Free,
    Used,
}

/// SWIOTLB 弹跳缓冲区
pub struct Swiotlb {
    /// 缓冲区物理基地址
    buffer_phys: u64,
    /// 缓冲区大小
    buffer_size: usize,
    /// 槽位数量
    nr_slots: usize,
    /// 槽位状态 (简化：使用位图)
    slot_bitmap: [u64; MAX_SLOTS / 64],
    /// 已使用槽位数
    used_slots: AtomicUsize,
    /// 下一个检查位置 (简单轮询分配)
    next_slot: AtomicUsize,
}

impl Swiotlb {
    /// 创建新的 SWIOTLB
    pub fn new(size: usize) -> Self {
        // 分配弹跳缓冲区
        let nr_pages = size / PAGE_SIZE as usize;
        let nr_slots = nr_pages.min(MAX_SLOTS);
        
        // 从低端内存分配 (< 4GB)
        let buffer_phys = super::super::memblock::memblock_alloc_range(
            size as u64,
            PAGE_SIZE,
            0,
            0x1_0000_0000, // 4GB 以下
        );
        
        Self {
            buffer_phys,
            buffer_size: size,
            nr_slots,
            slot_bitmap: [0u64; MAX_SLOTS / 64],
            used_slots: AtomicUsize::new(0),
            next_slot: AtomicUsize::new(0),
        }
    }
    
    /// 映射物理地址到 DMA 地址
    /// 
    /// 如果物理地址 < 4GB，直接返回；
    /// 否则，从弹跳缓冲区分配空间并复制数据
    pub fn map(&mut self, phys_addr: u64, size: usize) -> DmaAddr {
        // 如果已经在 32 位地址空间内，直接使用
        if phys_addr + size as u64 <= 0x1_0000_0000 {
            return DmaAddr::new(phys_addr);
        }
        
        // 需要使用弹跳缓冲区
        let slots_needed = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        
        if let Some(slot) = self.alloc_slots(slots_needed) {
            let dma_addr = self.buffer_phys + (slot as u64) * PAGE_SIZE;
            
            // 复制数据到弹跳缓冲区
            // TODO: 实现数据复制
            
            DmaAddr::new(dma_addr)
        } else {
            // 分配失败，返回原地址（可能导致 DMA 错误）
            DmaAddr::new(phys_addr)
        }
    }
    
    /// 取消映射
    pub fn unmap(&mut self, dma_addr: DmaAddr, size: usize) {
        let addr = dma_addr.as_u64();
        
        // 检查是否在弹跳缓冲区范围内
        if addr < self.buffer_phys || addr >= self.buffer_phys + self.buffer_size as u64 {
            return;
        }
        
        let slot = ((addr - self.buffer_phys) / PAGE_SIZE) as usize;
        let slots = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        
        self.free_slots(slot, slots);
    }
    
    /// 分配连续槽位
    fn alloc_slots(&mut self, count: usize) -> Option<usize> {
        if count == 0 || count > self.nr_slots {
            return None;
        }
        
        // 简单的首次适配算法
        let start = self.next_slot.load(Ordering::Relaxed) % self.nr_slots;
        let mut checked = 0;
        let mut i = start;
        
        while checked < self.nr_slots {
            // 检查从 i 开始的 count 个槽位是否空闲
            let mut all_free = true;
            for j in 0..count {
                let slot = (i + j) % self.nr_slots;
                if self.is_slot_used(slot) {
                    all_free = false;
                    i = (slot + 1) % self.nr_slots;
                    checked += j + 1;
                    break;
                }
            }
            
            if all_free {
                // 标记为已使用
                for j in 0..count {
                    let slot = (i + j) % self.nr_slots;
                    self.set_slot_used(slot, true);
                }
                self.used_slots.fetch_add(count, Ordering::Relaxed);
                self.next_slot.store((i + count) % self.nr_slots, Ordering::Relaxed);
                return Some(i);
            }
        }
        
        None
    }
    
    /// 释放槽位
    fn free_slots(&mut self, start: usize, count: usize) {
        for i in 0..count {
            let slot = (start + i) % self.nr_slots;
            self.set_slot_used(slot, false);
        }
        self.used_slots.fetch_sub(count, Ordering::Relaxed);
    }
    
    /// 检查槽位是否已使用
    fn is_slot_used(&self, slot: usize) -> bool {
        let word = slot / 64;
        let bit = slot % 64;
        (self.slot_bitmap[word] & (1 << bit)) != 0
    }
    
    /// 设置槽位状态
    fn set_slot_used(&mut self, slot: usize, used: bool) {
        let word = slot / 64;
        let bit = slot % 64;
        if used {
            self.slot_bitmap[word] |= 1 << bit;
        } else {
            self.slot_bitmap[word] &= !(1 << bit);
        }
    }
}
