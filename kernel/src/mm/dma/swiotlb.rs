// ============================================================================
// SWIOTLB (Software I/O TLB) 实现
//
// 当没有硬件 IOMMU 时使用的软件弹跳缓冲区
// 用于 32 位设备访问高地址内存
// ============================================================================

use super::{DmaAddr, DmaDirection, DMA_ADDR_SIZE, PAGE_SIZE};
use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::{AtomicUsize, Ordering};

/// SWIOTLB 最大槽位数 (64MB / 4KB = 16384)
const MAX_SLOTS: usize = 16384;

/// 槽位元数据（仅在起始槽位有效）
#[derive(Clone, Copy)]
struct SlotMeta {
    /// 原始物理地址
    orig_phys: u64,
    /// 映射大小（字节）
    size: usize,
    /// 占用槽位数
    slots: usize,
    /// DMA 方向
    dir: DmaDirection,
    /// 是否有效
    valid: bool,
}

impl SlotMeta {
    const fn empty() -> Self {
        Self {
            orig_phys: 0,
            size: 0,
            slots: 0,
            dir: DmaDirection::None,
            valid: false,
        }
    }
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
    slot_bitmap: Box<[u64]>,
    /// 槽位元数据（仅起始槽位使用）
    slot_meta: Box<[SlotMeta]>,
    /// 已使用槽位数
    used_slots: AtomicUsize,
    /// 下一个检查位置 (简单轮询分配)
    next_slot: AtomicUsize,
    /// 直接映射偏移
    direct_map_offset: u64,
}

impl Swiotlb {
    /// 在低端内存(<4GB)分配连续弹跳缓冲区。
    ///
    /// Buddy 最大块受 `MAX_ORDER` 限制，这里按可用 order 从大到小回退。
    fn alloc_lowmem_buffer(requested_slots: usize) -> (u64, usize) {
        if requested_slots == 0 {
            return (0, 0);
        }

        let max_order = crate::mm::MAX_ORDER.saturating_sub(1);
        let wanted_pages = requested_slots.next_power_of_two();
        let wanted_order = (wanted_pages.trailing_zeros() as usize).min(max_order);

        for order in (0..=wanted_order).rev() {
            if let Some(page) = crate::mm::alloc_pages(order, crate::mm::GFP_DMA32) {
                let pages = 1usize << order;
                let phys = crate::mm::page_to_pfn(page) * PAGE_SIZE;
                return (phys, pages.min(requested_slots));
            }
        }

        (0, 0)
    }

    /// 创建新的 SWIOTLB
    pub fn new(size: usize, direct_map_offset: u64) -> Self {
        // 分配弹跳缓冲区
        let nr_pages = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        let requested_slots = nr_pages.min(MAX_SLOTS);
        let (buffer_phys, nr_slots) = Self::alloc_lowmem_buffer(requested_slots);
        let buffer_size = nr_slots * PAGE_SIZE as usize;

        Self {
            buffer_phys,
            buffer_size,
            nr_slots,
            slot_bitmap: vec![0u64; MAX_SLOTS / 64].into_boxed_slice(),
            slot_meta: vec![SlotMeta::empty(); MAX_SLOTS].into_boxed_slice(),
            used_slots: AtomicUsize::new(0),
            next_slot: AtomicUsize::new(0),
            direct_map_offset,
        }
    }

    /// 映射物理地址到 DMA 地址
    ///
    /// 如果物理地址 < 4GB，直接返回；
    /// 否则，从弹跳缓冲区分配空间并复制数据
    pub fn map(&mut self, phys_addr: u64, size: usize, dir: DmaDirection) -> DmaAddr {
        if size == 0 {
            return DmaAddr::new(phys_addr);
        }

        // 如果已经在 32 位地址空间内，直接使用
        if phys_addr
            .checked_add(size as u64)
            .is_some_and(|end| end <= DMA_ADDR_SIZE)
        {
            return DmaAddr::new(phys_addr);
        }

        if self.nr_slots == 0 || self.buffer_phys == 0 {
            return DmaAddr::NULL;
        }

        // 需要使用弹跳缓冲区
        let slots_needed = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        if slots_needed == 0 || slots_needed > self.nr_slots {
            return DmaAddr::NULL;
        }

        if let Some(slot) = self.alloc_slots(slots_needed) {
            let dma_addr = self.buffer_phys + (slot as u64) * PAGE_SIZE;

            // 记录映射元数据（仅起始槽位）
            self.slot_meta[slot] = SlotMeta {
                orig_phys: phys_addr,
                size,
                slots: slots_needed,
                dir,
                valid: true,
            };

            // CPU -> Device 方向需要在 map 时复制
            if matches!(dir, DmaDirection::ToDevice | DmaDirection::Bidirectional) {
                unsafe {
                    self.copy_between_phys(dma_addr, phys_addr, size);
                }
            }

            DmaAddr::new(dma_addr)
        } else {
            DmaAddr::NULL
        }
    }

    /// 取消映射
    pub fn unmap(&mut self, dma_addr: DmaAddr, size: usize, dir: DmaDirection) {
        let addr = dma_addr.as_u64();

        // 检查是否在弹跳缓冲区范围内
        if addr < self.buffer_phys || addr >= self.buffer_phys + self.buffer_size as u64 {
            return;
        }

        let slot = ((addr - self.buffer_phys) / PAGE_SIZE) as usize;
        if slot >= self.nr_slots {
            return;
        }

        let meta = self.slot_meta[slot];
        if !meta.valid {
            // 兜底：按调用者传入的 size 尝试释放
            let slots = (size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
            self.free_slots(slot, slots);
            return;
        }

        // Device -> CPU 方向需要在 unmap 时回拷
        let need_copy_back =
            matches!(
                meta.dir,
                DmaDirection::FromDevice | DmaDirection::Bidirectional
            ) || matches!(dir, DmaDirection::FromDevice | DmaDirection::Bidirectional);

        if need_copy_back {
            let copy_size = core::cmp::min(meta.size, size);
            unsafe {
                self.copy_between_phys(meta.orig_phys, addr, copy_size);
            }
        }

        self.free_slots(slot, meta.slots);

        // 清理元数据
        for i in 0..meta.slots {
            let idx = slot + i;
            if idx < self.nr_slots {
                self.slot_meta[idx] = SlotMeta::empty();
            }
        }
    }

    /// 分配连续槽位
    fn alloc_slots(&mut self, count: usize) -> Option<usize> {
        if count == 0 || count > self.nr_slots {
            return None;
        }

        let start = self.next_slot.load(Ordering::Relaxed) % self.nr_slots;

        if let Some(slot) = self.find_contiguous_slots(start, count).or_else(|| {
            if start > 0 {
                self.find_contiguous_slots(0, count)
            } else {
                None
            }
        }) {
            for i in 0..count {
                self.set_slot_used(slot + i, true);
            }
            self.used_slots.fetch_add(count, Ordering::Relaxed);
            self.next_slot
                .store((slot + count) % self.nr_slots.max(1), Ordering::Relaxed);
            return Some(slot);
        }

        None
    }

    /// 释放槽位
    fn free_slots(&mut self, start: usize, count: usize) {
        if count == 0 || start >= self.nr_slots {
            return;
        }

        let max = core::cmp::min(start + count, self.nr_slots);
        for slot in start..max {
            self.set_slot_used(slot, false);
        }
        self.used_slots.fetch_sub(max - start, Ordering::Relaxed);
    }

    /// 查找连续空闲槽位（不允许跨尾首回绕）
    fn find_contiguous_slots(&self, start: usize, count: usize) -> Option<usize> {
        if count == 0 || count > self.nr_slots || start >= self.nr_slots {
            return None;
        }

        let last_start = self.nr_slots - count;
        if start > last_start {
            return None;
        }

        for base in start..=last_start {
            let mut all_free = true;
            for i in 0..count {
                if self.is_slot_used(base + i) {
                    all_free = false;
                    break;
                }
            }

            if all_free {
                return Some(base);
            }
        }

        None
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

    /// 物理地址转虚拟地址（直接映射）
    #[inline]
    fn phys_to_virt(&self, phys: u64) -> u64 {
        phys + self.direct_map_offset
    }

    /// 物理地址间复制（通过直接映射访问）
    unsafe fn copy_between_phys(&self, dst_phys: u64, src_phys: u64, size: usize) {
        let dst = self.phys_to_virt(dst_phys) as *mut u8;
        let src = self.phys_to_virt(src_phys) as *const u8;
        core::ptr::copy_nonoverlapping(src, dst, size);
    }

    /// 已使用槽位数（用于调试/测试）
    pub fn used_slots(&self) -> usize {
        self.used_slots.load(Ordering::Relaxed)
    }
}
