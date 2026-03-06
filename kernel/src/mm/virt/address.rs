//! 物理地址和虚拟地址类型
//!
//! 提供类型安全的地址操作。

use super::layout::PAGE_SIZE;
use core::fmt;

// =============================================================================
// 物理地址
// =============================================================================

/// 物理内存地址
///
/// 表示物理 RAM 中的地址，用于页表映射和 DMA 操作。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    /// 创建新的物理地址
    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// 零地址
    pub const fn zero() -> Self {
        Self(0)
    }

    /// 获取原始地址值
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// 检查地址是否对齐到页边界
    #[inline]
    pub const fn is_page_aligned(&self) -> bool {
        self.0 & (PAGE_SIZE - 1) == 0
    }

    /// 向下对齐到页边界
    #[inline]
    pub const fn page_align_down(&self) -> Self {
        Self(self.0 & !(PAGE_SIZE - 1))
    }

    /// 向上对齐到页边界
    #[inline]
    pub const fn page_align_up(&self) -> Self {
        Self((self.0 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1))
    }

    /// 获取页内偏移
    #[inline]
    pub const fn page_offset(&self) -> u64 {
        self.0 & (PAGE_SIZE - 1)
    }

    /// 转换为直接映射区的虚拟地址
    #[inline]
    pub fn to_virt(&self) -> VirtAddr {
        VirtAddr::new(crate::mm::phys_to_virt(self.0))
    }

    /// 地址加法
    #[inline]
    pub const fn add(&self, offset: u64) -> Self {
        Self(self.0 + offset)
    }

    /// 地址减法
    #[inline]
    pub const fn sub(&self, offset: u64) -> Self {
        Self(self.0 - offset)
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl From<u64> for PhysAddr {
    fn from(addr: u64) -> Self {
        Self::new(addr)
    }
}

impl From<PhysAddr> for u64 {
    fn from(addr: PhysAddr) -> Self {
        addr.0
    }
}

// =============================================================================
// 虚拟地址
// =============================================================================

/// 虚拟内存地址
///
/// 表示虚拟地址空间中的地址，需要通过页表转换为物理地址。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    #[inline]
    const fn canonicalize_48(addr: u64) -> u64 {
        if addr & (1 << 47) != 0 {
            addr | 0xFFFF_0000_0000_0000
        } else {
            addr & 0x0000_FFFF_FFFF_FFFF
        }
    }

    #[inline]
    fn canonicalize_runtime(addr: u64) -> u64 {
        let va_bits = crate::mm::va_bits();
        if va_bits == 57 {
            if addr & (1 << 56) != 0 {
                addr | (!0u64 << 57)
            } else {
                addr & ((1u64 << 57) - 1)
            }
        } else {
            Self::canonicalize_48(addr)
        }
    }

    /// 创建新的虚拟地址
    ///
    /// x86_64 要求虚拟地址符合规范形式（canonical form）。
    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(Self::canonicalize_48(addr))
    }

    /// 按当前运行时页表模式（48/57-bit）创建虚拟地址。
    #[inline]
    pub fn new_runtime(addr: u64) -> Self {
        Self(Self::canonicalize_runtime(addr))
    }

    /// 创建新的虚拟地址（不检查）
    #[inline]
    pub const fn new_unchecked(addr: u64) -> Self {
        Self(addr)
    }

    /// 零地址
    pub const fn zero() -> Self {
        Self(0)
    }

    /// 获取原始地址值
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// 获取为可变指针
    #[inline]
    pub const fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }

    /// 获取为不可变指针
    #[inline]
    pub const fn as_ptr<T>(&self) -> *const T {
        self.0 as *const T
    }

    /// 检查地址是否对齐到页边界
    #[inline]
    pub const fn is_page_aligned(&self) -> bool {
        self.0 & (PAGE_SIZE - 1) == 0
    }

    /// 向下对齐到页边界
    #[inline]
    pub const fn page_align_down(&self) -> Self {
        Self(self.0 & !(PAGE_SIZE - 1))
    }

    /// 向上对齐到页边界
    #[inline]
    pub fn page_align_up(&self) -> Self {
        Self::new_runtime((self.0 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1))
    }

    /// 获取页内偏移
    #[inline]
    pub const fn page_offset(&self) -> u64 {
        self.0 & (PAGE_SIZE - 1)
    }

    /// 地址加法
    #[inline]
    pub fn add(&self, offset: u64) -> Self {
        Self::new_runtime(self.0 + offset)
    }

    /// 地址减法
    #[inline]
    pub fn sub(&self, offset: u64) -> Self {
        Self::new_runtime(self.0 - offset)
    }

    // =========================================================================
    // 页表索引提取（用于页表遍历）
    // =========================================================================

    /// 获取 PML4 索引 (bits 39-47)
    #[inline]
    pub const fn pml4_index(&self) -> usize {
        ((self.0 >> 39) & 0x1FF) as usize
    }

    /// 获取 PDPT 索引 (bits 30-38)
    #[inline]
    pub const fn pdpt_index(&self) -> usize {
        ((self.0 >> 30) & 0x1FF) as usize
    }

    /// 获取 PD 索引 (bits 21-29)
    #[inline]
    pub const fn pd_index(&self) -> usize {
        ((self.0 >> 21) & 0x1FF) as usize
    }

    /// 获取 PT 索引 (bits 12-20)
    #[inline]
    pub const fn pt_index(&self) -> usize {
        ((self.0 >> 12) & 0x1FF) as usize
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl From<u64> for VirtAddr {
    fn from(addr: u64) -> Self {
        Self::new_runtime(addr)
    }
}

impl From<VirtAddr> for u64 {
    fn from(addr: VirtAddr) -> Self {
        addr.0
    }
}

impl<T> From<*const T> for VirtAddr {
    fn from(ptr: *const T) -> Self {
        Self::new_runtime(ptr as u64)
    }
}

impl<T> From<*mut T> for VirtAddr {
    fn from(ptr: *mut T) -> Self {
        Self::new_runtime(ptr as u64)
    }
}

// =============================================================================
// 物理页帧
// =============================================================================

/// 物理页帧
///
/// 表示一个 4KB 对齐的物理内存页。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysFrame(u64);

impl PhysFrame {
    /// 页帧大小 (4 KB)
    pub const SIZE: u64 = PAGE_SIZE;

    /// 从物理地址创建页帧（地址必须页对齐）
    #[inline]
    pub const fn from_addr(addr: PhysAddr) -> Option<Self> {
        if addr.is_page_aligned() {
            Some(Self(addr.as_u64() / PAGE_SIZE))
        } else {
            None
        }
    }

    /// 从物理地址创建页帧（向下对齐）
    #[inline]
    pub const fn containing_addr(addr: PhysAddr) -> Self {
        Self(addr.as_u64() / PAGE_SIZE)
    }

    /// 从页帧号创建
    #[inline]
    pub const fn from_pfn(pfn: u64) -> Self {
        Self(pfn)
    }

    /// 获取页帧号
    #[inline]
    pub const fn pfn(&self) -> u64 {
        self.0
    }

    /// 获取页帧起始物理地址
    #[inline]
    pub const fn start_addr(&self) -> PhysAddr {
        PhysAddr::new(self.0 * PAGE_SIZE)
    }

    /// 获取下一个页帧
    #[inline]
    pub const fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    /// 获取页帧范围内的地址数
    #[inline]
    pub const fn size(&self) -> u64 {
        PAGE_SIZE
    }
}

impl fmt::Debug for PhysFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PhysFrame(pfn={}, addr={:#x})",
            self.0,
            self.0 * PAGE_SIZE
        )
    }
}

// =============================================================================
// 页帧范围迭代器
// =============================================================================

/// 页帧范围
pub struct PhysFrameRange {
    start: PhysFrame,
    end: PhysFrame,
}

impl PhysFrameRange {
    /// 创建页帧范围
    pub const fn new(start: PhysFrame, end: PhysFrame) -> Self {
        Self { start, end }
    }

    /// 范围是否为空
    pub const fn is_empty(&self) -> bool {
        self.start.0 >= self.end.0
    }

    /// 范围中的页帧数量
    pub const fn count(&self) -> u64 {
        if self.end.0 > self.start.0 {
            self.end.0 - self.start.0
        } else {
            0
        }
    }
}

impl Iterator for PhysFrameRange {
    type Item = PhysFrame;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start.0 < self.end.0 {
            let frame = self.start;
            self.start = self.start.next();
            Some(frame)
        } else {
            None
        }
    }
}
