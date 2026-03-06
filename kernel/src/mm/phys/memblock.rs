// ============================================================================
// january_os - Memblock 早期内存分配器
//
// 类似 Linux 的 memblock，用于内核早期启动阶段的内存管理。
// 在 Buddy 系统初始化之前提供内存分配服务。
// ============================================================================
//!
//! # Memblock 概述
//!
//! Memblock 维护两个区域列表：
//! - `memory`: 所有物理内存区域（可用 + 已保留）
//! - `reserved`: 已保留的内存区域（内核、设备、已分配等）
//!
//! 分配逻辑：从 memory 中找到不在 reserved 中的区域进行分配，
//! 分配后将该区域添加到 reserved。
//!
//! ```text
//! memory:   [0 ---------> max_phys]
//! reserved: [kernel][page_array][heap][...]
//! free:     memory - reserved
//! ```
//!
//! # 生命周期
//!
//! 1. Bootloader 调用 `memblock_add()` 添加可用内存区域
//! 2. Bootloader 调用 `memblock_reserve()` 保留内核占用的内存
//! 3. 内核使用 `memblock_alloc()` 分配早期结构（page array, zones 等）
//! 4. Buddy 初始化时，memblock 将剩余空闲内存释放给 Buddy
//! 5. Memblock 不再使用（但保留的内存仍然有效）

use crate::error::{KernelError, KernelResult};
use core::cell::UnsafeCell;
use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// 常量配置
// ============================================================================

/// 最大内存区域数量
const MEMBLOCK_REGIONS_MAX: usize = 128;

/// 最大保留区域数量
const MEMBLOCK_RESERVED_MAX: usize = 128;

// ============================================================================
// 数据结构
// ============================================================================

/// 内存区域
#[derive(Debug, Clone, Copy)]
pub struct MemblockRegion {
    /// 区域起始物理地址
    pub base: u64,
    /// 区域大小（字节）
    pub size: u64,
    /// 区域标志
    pub flags: MemblockFlags,
}

impl MemblockRegion {
    /// 创建空区域
    pub const fn empty() -> Self {
        Self {
            base: 0,
            size: 0,
            flags: MemblockFlags::NONE,
        }
    }

    /// 检查区域是否为空
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// 获取区域结束地址
    #[inline]
    pub const fn end(&self) -> u64 {
        self.base + self.size
    }

    /// 检查是否与另一个区域重叠
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.base < other.end() && other.base < self.end()
    }

    /// 检查是否与另一个区域相邻（可合并）
    #[inline]
    pub fn adjacent(&self, other: &Self) -> bool {
        self.end() == other.base || other.end() == self.base
    }
}

/// 区域标志
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MemblockFlags {
    /// 无特殊标志
    NONE = 0,
    /// 热插拔内存
    HOTPLUG = 1 << 0,
    /// 镜像内存
    MIRROR = 1 << 1,
    /// 不可映射
    NOMAP = 1 << 2,
    /// 驱动管理
    DRIVER_MANAGED = 1 << 3,
}

/// 区域类型（用于内部管理）
#[derive(Debug, Clone, Copy)]
pub struct MemblockType {
    /// 区域数量
    pub cnt: usize,
    /// 最大区域数量
    pub max: usize,
    /// 总大小
    pub total_size: u64,
    /// 区域数组
    pub regions: [MemblockRegion; MEMBLOCK_REGIONS_MAX],
    /// 类型名称（调试用）
    pub name: &'static str,
}

impl MemblockType {
    /// 创建空的区域类型
    pub const fn new(name: &'static str, max: usize) -> Self {
        Self {
            cnt: 0,
            max,
            total_size: 0,
            regions: [MemblockRegion::empty(); MEMBLOCK_REGIONS_MAX],
            name,
        }
    }
}

/// Memblock 主结构
pub struct Memblock {
    /// 是否允许从底部分配（默认 false，从顶部分配）
    pub bottom_up: bool,
    /// 当前内存分配限制
    pub current_limit: u64,
    /// 所有内存区域
    pub memory: MemblockType,
    /// 已保留的内存区域
    pub reserved: MemblockType,
}

// ============================================================================
// 全局 Memblock 实例
// ============================================================================

struct MemblockState {
    inner: UnsafeCell<Memblock>,
}

unsafe impl Sync for MemblockState {}

impl MemblockState {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Memblock {
                bottom_up: false,
                current_limit: u64::MAX,
                memory: MemblockType::new("memory", MEMBLOCK_REGIONS_MAX),
                reserved: MemblockType::new("reserved", MEMBLOCK_RESERVED_MAX),
            }),
        }
    }
}

/// 全局 memblock 实例
static MEMBLOCK: MemblockState = MemblockState::new();

/// Memblock 是否已初始化
static MEMBLOCK_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[inline]
fn memblock_ref() -> &'static Memblock {
    unsafe { &*MEMBLOCK.inner.get() }
}

#[inline]
unsafe fn memblock_mut() -> &'static mut Memblock {
    unsafe { &mut *MEMBLOCK.inner.get() }
}

// ============================================================================
// 公共 API
// ============================================================================

/// 初始化 memblock
pub fn memblock_init() {
    unsafe {
        let mb = memblock_mut();
        mb.memory.cnt = 0;
        mb.memory.total_size = 0;
        mb.reserved.cnt = 0;
        mb.reserved.total_size = 0;
        mb.bottom_up = false;
        mb.current_limit = u64::MAX;
        MEMBLOCK_INITIALIZED.store(true, Ordering::Release);
    }
}

/// 检查 memblock 是否已初始化
pub fn memblock_initialized() -> bool {
    MEMBLOCK_INITIALIZED.load(Ordering::Acquire)
}

/// 添加内存区域到 memory 列表
///
/// # Arguments
/// * `base` - 区域起始物理地址
/// * `size` - 区域大小
///
/// # Returns
/// 成功返回 Ok(()), 失败返回错误信息
pub fn memblock_add(base: u64, size: u64) -> KernelResult<()> {
    unsafe { memblock_add_range(&mut memblock_mut().memory, base, size, MemblockFlags::NONE) }
}

/// 保留内存区域
///
/// # Arguments
/// * `base` - 区域起始物理地址
/// * `size` - 区域大小
pub fn memblock_reserve(base: u64, size: u64) -> KernelResult<()> {
    unsafe {
        memblock_add_range(
            &mut memblock_mut().reserved,
            base,
            size,
            MemblockFlags::NONE,
        )
    }
}

/// 从 memblock 分配内存
///
/// # Arguments
/// * `size` - 需要分配的大小
/// * `align` - 对齐要求
///
/// # Returns
/// 成功返回物理地址，失败返回 0
pub fn memblock_alloc(size: u64, align: u64) -> u64 {
    memblock_alloc_range(size, align, 0, memblock_ref().current_limit)
}

/// 从指定范围分配内存
///
/// # Arguments
/// * `size` - 需要分配的大小
/// * `align` - 对齐要求
/// * `start` - 范围起始地址
/// * `end` - 范围结束地址
pub fn memblock_alloc_range(size: u64, align: u64, start: u64, end: u64) -> u64 {
    if size == 0 {
        return 0;
    }

    let align = if align == 0 { 1 } else { align };

    unsafe {
        let memblock = memblock_mut();

        // 根据 bottom_up 决定分配方向
        if memblock.bottom_up {
            memblock_find_in_range_bottom_up(memblock, size, align, start, end)
        } else {
            memblock_find_in_range_top_down(memblock, size, align, start, end)
        }
    }
}

/// 从 memblock 分配内存并清零
pub fn memblock_alloc_zeroed(size: u64, align: u64) -> u64 {
    let addr = memblock_alloc(size, align);
    if addr != 0 {
        // 通过直接映射清零
        let virt = phys_to_virt(addr);
        unsafe {
            core::ptr::write_bytes(virt as *mut u8, 0, size as usize);
        }
    }
    addr
}

/// 释放 memblock 中的保留内存
///
/// 注意：这只是从 reserved 列表中移除，不影响 memory 列表
pub fn memblock_free(base: u64, size: u64) -> KernelResult<()> {
    unsafe { memblock_remove_range(&mut memblock_mut().reserved, base, size) }
}

/// 设置分配方向
///
/// * `enable` - true 从底部分配，false 从顶部分配（默认）
pub fn memblock_set_bottom_up(enable: bool) {
    unsafe {
        memblock_mut().bottom_up = enable;
    }
}

/// 设置分配限制
pub fn memblock_set_current_limit(limit: u64) {
    unsafe {
        memblock_mut().current_limit = limit;
    }
}

/// 获取物理内存总大小
pub fn memblock_phys_mem_size() -> u64 {
    memblock_ref().memory.total_size
}

/// 获取已保留内存总大小
pub fn memblock_reserved_size() -> u64 {
    memblock_ref().reserved.total_size
}

/// 获取空闲内存大小
pub fn memblock_free_size() -> u64 {
    memblock_phys_mem_size().saturating_sub(memblock_reserved_size())
}

/// 获取最大物理地址
pub fn memblock_end_of_phys_mem() -> u64 {
    let memory = &memblock_ref().memory;
    if memory.cnt == 0 {
        0
    } else {
        memory.regions[memory.cnt - 1].end()
    }
}

/// 遍历所有空闲内存区域
///
/// 回调函数参数：(base, size) -> bool
/// 返回 false 停止遍历
pub fn memblock_for_each_free_region<F>(mut callback: F)
where
    F: FnMut(u64, u64) -> bool,
{
    let memblock = memblock_ref();

    for i in 0..memblock.memory.cnt {
        let mem_region = &memblock.memory.regions[i];
        if mem_region.is_empty() {
            continue;
        }

        let mut current = mem_region.base;
        let mem_end = mem_region.end();

        // 遍历该内存区域，排除已保留的部分
        for j in 0..memblock.reserved.cnt {
            let res_region = &memblock.reserved.regions[j];
            if res_region.is_empty() {
                continue;
            }

            // 检查保留区域是否在当前内存区域内
            if res_region.base >= mem_end {
                break;
            }
            if res_region.end() <= current {
                continue;
            }

            // 保留区域之前的部分是空闲的
            if current < res_region.base {
                let free_end = min(res_region.base, mem_end);
                if !callback(current, free_end - current) {
                    return;
                }
            }

            current = max(current, res_region.end());
        }

        // 处理最后一个保留区域之后的空闲部分
        if current < mem_end {
            if !callback(current, mem_end - current) {
                return;
            }
        }
    }
}

// ============================================================================
// 调试接口
// ============================================================================

/// 打印 memblock 状态
pub fn memblock_dump() {
    let memblock = memblock_ref();

    // 这里可以使用串口打印
    // 由于没有 kprintln 宏的直接访问，调用者需要自己打印
    let _ = memblock;
}

/// 获取 memory 区域数量
pub fn memblock_memory_region_count() -> usize {
    memblock_ref().memory.cnt
}

/// 获取 reserved 区域数量
pub fn memblock_reserved_region_count() -> usize {
    memblock_ref().reserved.cnt
}

/// 获取 memory 区域（用于迭代）
pub fn memblock_memory_region(index: usize) -> Option<MemblockRegion> {
    let mb = memblock_ref();
    if index < mb.memory.cnt {
        Some(mb.memory.regions[index])
    } else {
        None
    }
}

/// 获取 reserved 区域（用于迭代）
pub fn memblock_reserved_region(index: usize) -> Option<MemblockRegion> {
    let mb = memblock_ref();
    if index < mb.reserved.cnt {
        Some(mb.reserved.regions[index])
    } else {
        None
    }
}

// ============================================================================
// 内部实现
// ============================================================================

/// 添加区域到类型列表
fn memblock_add_range(
    type_: &mut MemblockType,
    base: u64,
    size: u64,
    flags: MemblockFlags,
) -> KernelResult<()> {
    if size == 0 {
        return Ok(());
    }

    let end = base.checked_add(size).ok_or(KernelError::InvalidAddress)?;

    // 查找插入位置并检查重叠
    let mut insert_idx = type_.cnt;
    for i in 0..type_.cnt {
        let region = &type_.regions[i];

        // 检查是否可以合并
        if region.base <= end && base <= region.end() {
            // 重叠或相邻，尝试合并
            return memblock_merge_regions(type_, base, size, flags);
        }

        if base < region.base {
            insert_idx = i;
            break;
        }
    }

    // 检查是否还有空间
    if type_.cnt >= type_.max {
        return Err(KernelError::NoMemory);
    }

    // 插入新区域
    // 移动后面的区域
    for i in (insert_idx..type_.cnt).rev() {
        type_.regions[i + 1] = type_.regions[i];
    }

    type_.regions[insert_idx] = MemblockRegion { base, size, flags };
    type_.cnt += 1;
    type_.total_size += size;

    Ok(())
}

/// 合并重叠的区域
fn memblock_merge_regions(
    type_: &mut MemblockType,
    base: u64,
    size: u64,
    _flags: MemblockFlags,
) -> KernelResult<()> {
    let end = base + size;

    for i in 0..type_.cnt {
        let region = &mut type_.regions[i];

        // 检查是否重叠或相邻
        if region.base <= end && base <= region.end() {
            let new_base = min(region.base, base);
            let new_end = max(region.end(), end);
            let old_size = region.size;

            region.base = new_base;
            region.size = new_end - new_base;

            // 更新总大小（增量）
            type_.total_size = type_.total_size - old_size + region.size;

            // 尝试与下一个区域合并
            memblock_merge_adjacent(type_, i);

            return Ok(());
        }
    }

    Err(KernelError::Failed)
}

/// 合并相邻区域
fn memblock_merge_adjacent(type_: &mut MemblockType, start_idx: usize) {
    let mut i = start_idx;
    while i + 1 < type_.cnt {
        let current_end = type_.regions[i].end();
        let next_base = type_.regions[i + 1].base;

        if current_end >= next_base {
            // 可以合并
            let new_end = max(current_end, type_.regions[i + 1].end());
            let old_size = type_.regions[i].size + type_.regions[i + 1].size;

            type_.regions[i].size = new_end - type_.regions[i].base;

            // 移除下一个区域
            for j in (i + 1)..(type_.cnt - 1) {
                type_.regions[j] = type_.regions[j + 1];
            }
            type_.cnt -= 1;

            // 更新总大小
            type_.total_size = type_.total_size - old_size + type_.regions[i].size;
        } else {
            i += 1;
        }
    }
}

/// 从类型列表中移除区域
fn memblock_remove_range(type_: &mut MemblockType, base: u64, size: u64) -> KernelResult<()> {
    if size == 0 {
        return Ok(());
    }

    let end = base + size;
    let mut i = 0;

    while i < type_.cnt {
        let region = &type_.regions[i];

        // 检查是否有重叠
        if region.base >= end || region.end() <= base {
            i += 1;
            continue;
        }

        // 有重叠，需要处理
        let reg_base = region.base;
        let reg_end = region.end();
        let reg_flags = region.flags;

        if base <= reg_base && end >= reg_end {
            // 完全移除这个区域
            type_.total_size -= region.size;
            for j in i..(type_.cnt - 1) {
                type_.regions[j] = type_.regions[j + 1];
            }
            type_.cnt -= 1;
            // 不增加 i，继续检查当前位置
        } else if base > reg_base && end < reg_end {
            // 中间打洞，需要分成两个区域
            if type_.cnt >= type_.max {
                return Err(KernelError::NoMemory);
            }

            // 修改当前区域为前半部分
            type_.regions[i].size = base - reg_base;
            type_.total_size -= size;

            // 插入后半部分
            for j in (i + 1..type_.cnt).rev() {
                type_.regions[j + 1] = type_.regions[j];
            }
            type_.regions[i + 1] = MemblockRegion {
                base: end,
                size: reg_end - end,
                flags: reg_flags,
            };
            type_.cnt += 1;
            i += 2;
        } else if base <= reg_base {
            // 移除前半部分
            let removed = end - reg_base;
            type_.regions[i].base = end;
            type_.regions[i].size -= removed;
            type_.total_size -= removed;
            i += 1;
        } else {
            // 移除后半部分
            let removed = reg_end - base;
            type_.regions[i].size -= removed;
            type_.total_size -= removed;
            i += 1;
        }
    }

    Ok(())
}

/// 从顶部向下查找空闲区域
fn memblock_find_in_range_top_down(
    memblock: &mut Memblock,
    size: u64,
    align: u64,
    start: u64,
    end: u64,
) -> u64 {
    // 从最后一个内存区域开始向前搜索
    for i in (0..memblock.memory.cnt).rev() {
        let mem_region = &memblock.memory.regions[i];
        if mem_region.is_empty() {
            continue;
        }

        // 计算这个区域的搜索范围
        let region_start = max(mem_region.base, start);
        let region_end = min(mem_region.end(), end);

        if region_start >= region_end {
            continue;
        }

        // 尝试在这个区域中分配
        if let Some(addr) = find_free_area_top_down(memblock, size, align, region_start, region_end)
        {
            // 找到了，保留这块内存
            if memblock_add_range(&mut memblock.reserved, addr, size, MemblockFlags::NONE).is_ok() {
                return addr;
            }
        }
    }

    0
}

/// 从底部向上查找空闲区域
fn memblock_find_in_range_bottom_up(
    memblock: &mut Memblock,
    size: u64,
    align: u64,
    start: u64,
    end: u64,
) -> u64 {
    // 从第一个内存区域开始向后搜索
    for i in 0..memblock.memory.cnt {
        let mem_region = &memblock.memory.regions[i];
        if mem_region.is_empty() {
            continue;
        }

        // 计算这个区域的搜索范围
        let region_start = max(mem_region.base, start);
        let region_end = min(mem_region.end(), end);

        if region_start >= region_end {
            continue;
        }

        // 尝试在这个区域中分配
        if let Some(addr) =
            find_free_area_bottom_up(memblock, size, align, region_start, region_end)
        {
            // 找到了，保留这块内存
            if memblock_add_range(&mut memblock.reserved, addr, size, MemblockFlags::NONE).is_ok() {
                return addr;
            }
        }
    }

    0
}

/// 在指定范围内从顶部向下查找空闲区域
fn find_free_area_top_down(
    memblock: &Memblock,
    size: u64,
    align: u64,
    start: u64,
    end: u64,
) -> Option<u64> {
    // 从顶部开始，对齐后的地址
    let mut candidate = align_down(end.saturating_sub(size), align);

    while candidate >= start {
        // 检查是否与任何保留区域冲突
        let mut conflict = false;
        let candidate_end = candidate + size;

        for i in 0..memblock.reserved.cnt {
            let res = &memblock.reserved.regions[i];
            if res.is_empty() {
                continue;
            }

            // 检查冲突
            if candidate < res.end() && candidate_end > res.base {
                // 有冲突，跳过这个保留区域
                if res.base > 0 {
                    candidate = align_down(res.base.saturating_sub(size), align);
                } else {
                    return None;
                }
                conflict = true;
                break;
            }
        }

        if !conflict {
            return Some(candidate);
        }

        if candidate == 0 {
            break;
        }
    }

    None
}

/// 在指定范围内从底部向上查找空闲区域
fn find_free_area_bottom_up(
    memblock: &Memblock,
    size: u64,
    align: u64,
    start: u64,
    end: u64,
) -> Option<u64> {
    // 从底部开始，对齐后的地址
    let mut candidate = align_up(start, align);

    while candidate + size <= end {
        // 检查是否与任何保留区域冲突
        let mut conflict = false;
        let candidate_end = candidate + size;

        for i in 0..memblock.reserved.cnt {
            let res = &memblock.reserved.regions[i];
            if res.is_empty() {
                continue;
            }

            // 检查冲突
            if candidate < res.end() && candidate_end > res.base {
                // 有冲突，跳到这个保留区域之后
                candidate = align_up(res.end(), align);
                conflict = true;
                break;
            }
        }

        if !conflict {
            return Some(candidate);
        }
    }

    None
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 向上对齐
#[inline]
const fn align_up(addr: u64, align: u64) -> u64 {
    (addr + align - 1) & !(align - 1)
}

/// 向下对齐
#[inline]
const fn align_down(addr: u64, align: u64) -> u64 {
    addr & !(align - 1)
}

/// 物理地址转虚拟地址（通过直接映射）
#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    crate::mm::phys_to_virt(phys)
}
