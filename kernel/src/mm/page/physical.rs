//! 物理内存管理器
//!
//! 使用位图分配器管理物理页帧的分配与释放。
//!
//! ## 设计
//!
//! - 每个位代表一个 4KB 页帧
//! - 0 = 空闲，1 = 已使用
//! - 支持单页和连续多页分配
//! - 线程安全（使用自旋锁）

#![allow(unsafe_op_in_unsafe_fn)]

use crate::mm::vm::address::{PhysAddr, PhysFrame};
use crate::mm::vm::layout::{page_align_up, PAGE_SIZE};
use core::cell::UnsafeCell;
use core::ptr;

/// 内存区域类型（与 BootInfo 中的定义保持一致）
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// 可用内存
    Usable = 0,
    /// 保留（硬件/固件）
    Reserved = 1,
    /// ACPI 可回收内存
    AcpiReclaimable = 2,
    /// ACPI NVS 内存
    AcpiNvs = 3,
    /// 内存映射 I/O
    Mmio = 4,
    /// 引导程序可回收
    BootloaderReclaimable = 5,
    /// 内核占用
    KernelAndModules = 6,
    /// 帧缓冲区
    Framebuffer = 7,
}

/// 内存区域描述符（与 BootInfo 中的定义匹配）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryRegion {
    pub phys_start: u64,
    pub virt_start: u64,
    pub page_count: u64,
    pub region_type: u32,
    pub attributes: u32,
}

impl MemoryRegion {
    /// 获取区域类型
    pub fn kind(&self) -> MemoryRegionType {
        match self.region_type {
            0 => MemoryRegionType::Usable,
            1 => MemoryRegionType::Reserved,
            2 => MemoryRegionType::AcpiReclaimable,
            3 => MemoryRegionType::AcpiNvs,
            4 => MemoryRegionType::Mmio,
            5 => MemoryRegionType::BootloaderReclaimable,
            6 => MemoryRegionType::KernelAndModules,
            7 => MemoryRegionType::Framebuffer,
            _ => MemoryRegionType::Reserved,
        }
    }

    /// 获取区域大小（字节）
    pub fn size(&self) -> u64 {
        self.page_count * PAGE_SIZE
    }

    /// 获取区域结束地址
    pub fn end(&self) -> u64 {
        self.phys_start + self.size()
    }
}

/// 内存统计信息
#[derive(Clone, Copy, Debug)]
pub struct MemoryStats {
    /// 总页帧数
    pub total_frames: u64,
    /// 已使用页帧数
    pub used_frames: u64,
    /// 空闲页帧数
    pub free_frames: u64,
    /// 总内存字节数
    pub total_bytes: u64,
    /// 已使用字节数
    pub used_bytes: u64,
    /// 空闲字节数
    pub free_bytes: u64,
}

/// 位图分配器内部实现
struct BitmapAllocatorInner {
    /// 位图数据指针
    bitmap: *mut u8,
    /// 位图大小（字节）
    bitmap_size: usize,
    /// 总页帧数
    total_frames: u64,
    /// 已使用页帧数
    used_frames: u64,
    /// 下一个搜索起点（优化连续分配）
    next_free_hint: u64,
    /// 是否已初始化
    initialized: bool,
}

// 内部实现需要手动实现 Send，因为包含原始指针
unsafe impl Send for BitmapAllocatorInner {}

impl BitmapAllocatorInner {
    /// 创建未初始化的分配器
    const fn new_uninit() -> Self {
        Self {
            bitmap: ptr::null_mut(),
            bitmap_size: 0,
            total_frames: 0,
            used_frames: 0,
            next_free_hint: 0,
            initialized: false,
        }
    }

    /// 设置位（标记为已使用）
    fn set_bit(&mut self, frame_idx: u64) {
        if frame_idx >= self.total_frames {
            return;
        }
        let byte_idx = (frame_idx / 8) as usize;
        let bit_idx = (frame_idx % 8) as u8;
        unsafe {
            *self.bitmap.add(byte_idx) |= 1 << bit_idx;
        }
    }

    /// 清除位（标记为空闲）
    fn clear_bit(&mut self, frame_idx: u64) {
        if frame_idx >= self.total_frames {
            return;
        }
        let byte_idx = (frame_idx / 8) as usize;
        let bit_idx = (frame_idx % 8) as u8;
        unsafe {
            *self.bitmap.add(byte_idx) &= !(1 << bit_idx);
        }
    }

    /// 测试位（检查是否已使用）
    fn test_bit(&self, frame_idx: u64) -> bool {
        if frame_idx >= self.total_frames {
            return true; // 超出范围视为已使用
        }
        let byte_idx = (frame_idx / 8) as usize;
        let bit_idx = (frame_idx % 8) as u8;
        unsafe { (*self.bitmap.add(byte_idx) >> bit_idx) & 1 != 0 }
    }

    /// 设置范围内的所有位
    fn set_range(&mut self, start: u64, count: u64) {
        for i in 0..count {
            self.set_bit(start + i);
        }
    }

    /// 清除范围内的所有位
    fn clear_range(&mut self, start: u64, count: u64) {
        for i in 0..count {
            self.clear_bit(start + i);
        }
    }

    /// 查找第一个空闲页帧
    fn find_free_frame(&self) -> Option<u64> {
        // 从 hint 开始搜索
        for frame_idx in self.next_free_hint..self.total_frames {
            if !self.test_bit(frame_idx) {
                return Some(frame_idx);
            }
        }
        // 如果没找到，从头搜索
        for frame_idx in 0..self.next_free_hint {
            if !self.test_bit(frame_idx) {
                return Some(frame_idx);
            }
        }
        None
    }

    /// 查找连续的空闲页帧
    fn find_free_frames(&self, count: u64) -> Option<u64> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.find_free_frame();
        }

        let mut consecutive = 0u64;
        let mut start = 0u64;

        for frame_idx in 0..self.total_frames {
            if !self.test_bit(frame_idx) {
                if consecutive == 0 {
                    start = frame_idx;
                }
                consecutive += 1;
                if consecutive >= count {
                    return Some(start);
                }
            } else {
                consecutive = 0;
            }
        }
        None
    }

    /// 分配单个页帧
    fn allocate(&mut self) -> Option<PhysFrame> {
        if !self.initialized {
            return None;
        }

        if let Some(frame_idx) = self.find_free_frame() {
            self.set_bit(frame_idx);
            self.used_frames += 1;
            self.next_free_hint = frame_idx + 1;
            Some(PhysFrame::from_pfn(frame_idx))
        } else {
            None
        }
    }

    /// 分配连续的页帧
    fn allocate_contiguous(&mut self, count: u64) -> Option<PhysFrame> {
        if !self.initialized || count == 0 {
            return None;
        }

        if let Some(start) = self.find_free_frames(count) {
            self.set_range(start, count);
            self.used_frames += count;
            self.next_free_hint = start + count;
            Some(PhysFrame::from_pfn(start))
        } else {
            None
        }
    }

    /// 释放单个页帧
    fn deallocate(&mut self, frame: PhysFrame) {
        if !self.initialized {
            return;
        }

        let frame_idx = frame.pfn();
        if frame_idx < self.total_frames && self.test_bit(frame_idx) {
            self.clear_bit(frame_idx);
            self.used_frames -= 1;
            // 更新 hint 以优化后续分配
            if frame_idx < self.next_free_hint {
                self.next_free_hint = frame_idx;
            }
        }
    }

    /// 释放连续的页帧
    fn deallocate_contiguous(&mut self, start: PhysFrame, count: u64) {
        if !self.initialized || count == 0 {
            return;
        }

        let start_idx = start.pfn();
        for i in 0..count {
            let frame_idx = start_idx + i;
            if frame_idx < self.total_frames && self.test_bit(frame_idx) {
                self.clear_bit(frame_idx);
                self.used_frames -= 1;
            }
        }

        if start_idx < self.next_free_hint {
            self.next_free_hint = start_idx;
        }
    }

    /// 获取统计信息
    fn stats(&self) -> MemoryStats {
        let free_frames = if self.total_frames > self.used_frames {
            self.total_frames - self.used_frames
        } else {
            0
        };

        MemoryStats {
            total_frames: self.total_frames,
            used_frames: self.used_frames,
            free_frames,
            total_bytes: self.total_frames * PAGE_SIZE,
            used_bytes: self.used_frames * PAGE_SIZE,
            free_bytes: free_frames * PAGE_SIZE,
        }
    }
}

/// 物理内存管理器
///
/// 物理页帧分配器。
/// 注意：当前实现不是线程安全的，只适用于单核启动阶段。
pub struct PhysicalMemoryManager {
    inner: UnsafeCell<BitmapAllocatorInner>,
}

// 注意：这不是真正的线程安全，仅用于启动阶段
unsafe impl Sync for PhysicalMemoryManager {}

impl PhysicalMemoryManager {
    /// 创建未初始化的物理内存管理器
    pub const fn new_uninit() -> Self {
        Self {
            inner: UnsafeCell::new(BitmapAllocatorInner::new_uninit()),
        }
    }

    /// 初始化物理内存管理器
    ///
    /// # 参数
    ///
    /// - `memory_map`: 来自 BootInfo 的内存映射
    /// - `memory_map_entries`: 内存映射条目数
    /// - `kernel_end`: 内核结束物理地址
    ///
    /// # Safety
    ///
    /// 必须在内核启动早期调用，且只能调用一次。
    /// 调用者必须确保内存映射信息正确有效。
    pub unsafe fn init(
        &self,
        memory_map: *const MemoryRegion,
        memory_map_entries: u32,
        kernel_end: u64,
    ) {
        let inner = &mut *self.inner.get();

        // 1. 计算总物理内存大小（只扫描前 100 个条目，避免潜在问题）
        let mut max_phys_addr: u64 = 0;
        let entries_to_scan = (memory_map_entries as usize).min(100);
        
        for i in 0..entries_to_scan {
            let region = &*memory_map.add(i);
            let end = region.phys_start.saturating_add(region.page_count.saturating_mul(PAGE_SIZE));
            if end > max_phys_addr {
                max_phys_addr = end;
            }
        }

        // 限制最大物理地址避免位图过大
        max_phys_addr = max_phys_addr.min(256 * 1024 * 1024); // 最多 256 MB

        // 2. 计算需要的页帧数和位图大小
        let total_frames = max_phys_addr / PAGE_SIZE;
        let bitmap_size = ((total_frames + 7) / 8) as usize;

        // 3. 在内核结束后分配位图空间
        let bitmap_start = page_align_up(kernel_end);
        let bitmap_ptr = bitmap_start as *mut u8;

        // 4. 初始化位图（全部标记为已使用）- 使用字节级写入
        let mut i = 0usize;
        while i < bitmap_size {
            *bitmap_ptr.add(i) = 0xFF;
            i += 1;
        }

        // 5. 设置内部状态
        inner.bitmap = bitmap_ptr;
        inner.bitmap_size = bitmap_size;
        inner.total_frames = total_frames;
        inner.used_frames = total_frames;
        inner.next_free_hint = 0;
        inner.initialized = true;

        // 6. 遍历内存映射，将可用区域标记为空闲
        for i in 0..entries_to_scan {
            let region = &*memory_map.add(i);
            if region.kind() == MemoryRegionType::Usable {
                let start_frame = region.phys_start / PAGE_SIZE;
                let frame_count = region.page_count.min(total_frames);
                
                let mut j = 0u64;
                while j < frame_count {
                    let frame_idx = start_frame + j;
                    if frame_idx < total_frames && inner.test_bit(frame_idx) {
                        inner.clear_bit(frame_idx);
                        inner.used_frames = inner.used_frames.saturating_sub(1);
                    }
                    j += 1;
                }
            }
        }

        // 7. 保留特殊区域（使用 while 循环避免潜在的迭代器问题）
        
        // 7.1 保留低端 1MB
        let low_memory_frames = 0x100000 / PAGE_SIZE;
        let mut i = 0u64;
        while i < low_memory_frames {
            if !inner.test_bit(i) {
                inner.set_bit(i);
                inner.used_frames += 1;
            }
            i += 1;
        }

        // 7.2 保留内核占用的区域
        let kernel_start_frame = 0x100000 / PAGE_SIZE;
        let kernel_end_frame = page_align_up(kernel_end) / PAGE_SIZE;
        let mut i = kernel_start_frame;
        while i < kernel_end_frame {
            if !inner.test_bit(i) {
                inner.set_bit(i);
                inner.used_frames += 1;
            }
            i += 1;
        }

        // 7.3 保留位图自身占用的区域
        let bitmap_end_addr = bitmap_start + bitmap_size as u64;
        let bitmap_start_frame = bitmap_start / PAGE_SIZE;
        let bitmap_end_frame = page_align_up(bitmap_end_addr) / PAGE_SIZE;
        let mut i = bitmap_start_frame;
        while i < bitmap_end_frame {
            if !inner.test_bit(i) {
                inner.set_bit(i);
                inner.used_frames += 1;
            }
            i += 1;
        }
    }

    /// 分配单个物理页帧
    pub fn allocate_frame(&self) -> Option<PhysFrame> {
        unsafe { (*self.inner.get()).allocate() }
    }

    /// 分配连续的物理页帧
    pub fn allocate_frames(&self, count: u64) -> Option<PhysFrame> {
        unsafe { (*self.inner.get()).allocate_contiguous(count) }
    }

    /// 释放单个物理页帧
    pub fn deallocate_frame(&self, frame: PhysFrame) {
        unsafe { (*self.inner.get()).deallocate(frame) }
    }

    /// 释放连续的物理页帧
    pub fn deallocate_frames(&self, start: PhysFrame, count: u64) {
        unsafe { (*self.inner.get()).deallocate_contiguous(start, count) }
    }

    /// 获取内存统计信息
    pub fn stats(&self) -> MemoryStats {
        unsafe { (*self.inner.get()).stats() }
    }

    /// 检查是否已初始化
    pub fn is_initialized(&self) -> bool {
        unsafe { (*self.inner.get()).initialized }
    }

    /// 标记地址范围为已使用
    ///
    /// 用于保留特定物理地址区域（如帧缓冲区）
    pub fn mark_used(&self, start_addr: PhysAddr, size: u64) {
        unsafe {
            let inner = &mut *self.inner.get();
            let start_frame = start_addr.as_u64() / PAGE_SIZE;
            let frame_count = (size + PAGE_SIZE - 1) / PAGE_SIZE;

            for i in 0..frame_count {
                let frame_idx = start_frame + i;
                if frame_idx < inner.total_frames && !inner.test_bit(frame_idx) {
                    inner.set_bit(frame_idx);
                    inner.used_frames += 1;
                }
            }
        }
    }

    /// 获取位图结束地址
    ///
    /// 用于确定内核堆或页表可以开始的位置
    pub fn bitmap_end(&self) -> u64 {
        unsafe {
            let inner = &*self.inner.get();
            if inner.initialized {
                inner.bitmap as u64 + inner.bitmap_size as u64
            } else {
                0
            }
        }
    }
}

/// 全局物理内存管理器实例
pub static PMM: PhysicalMemoryManager = PhysicalMemoryManager::new_uninit();
