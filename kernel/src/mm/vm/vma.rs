// ============================================================================
// january_os - VMA (Virtual Memory Area) 虚拟内存区域管理
//
// 参考 Linux vm_area_struct，管理进程的虚拟地址空间
// 使用 MapleTree 进行 O(log n) 的区间查找和间隙搜索
// ============================================================================

use super::layout::PAGE_SIZE;
use crate::libs::mptree::MapleTree;
use crate::sync::SpinLock;

// ============================================================================
// VMA 标志位
// ============================================================================

/// VMA 权限和属性标志
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct VmFlags(u64);

impl VmFlags {
    // ========== 基本权限 (与 mmap prot 对应) ==========
    /// 可读
    pub const READ: u64      = 1 << 0;
    /// 可写
    pub const WRITE: u64     = 1 << 1;
    /// 可执行
    pub const EXEC: u64      = 1 << 2;
    /// 共享映射 (vs 私有)
    pub const SHARED: u64    = 1 << 3;

    // ========== 映射类型 ==========
    /// 匿名映射 (无文件后备)
    pub const ANONYMOUS: u64 = 1 << 4;
    /// 栈区域 (向下增长)
    pub const GROWSDOWN: u64 = 1 << 5;
    /// 堆区域
    pub const HEAP: u64      = 1 << 6;
    /// 代码段
    pub const CODE: u64      = 1 << 7;
    /// 数据段
    pub const DATA: u64      = 1 << 8;
    /// BSS 段
    pub const BSS: u64       = 1 << 9;

    // ========== 特殊属性 ==========
    /// 锁定在内存中 (不可换出)
    pub const LOCKED: u64    = 1 << 10;
    /// IO 映射 (设备内存)
    pub const IO: u64        = 1 << 11;
    /// 巨页映射
    pub const HUGETLB: u64   = 1 << 12;
    /// 不可合并
    pub const DONTMERGE: u64 = 1 << 13;
    /// 不可扩展
    pub const DONTEXPAND: u64 = 1 << 14;
    /// 写时复制
    pub const MAYWRITE: u64  = 1 << 15;

    // ========== 常用组合 ==========
    pub const RW: u64 = Self::READ | Self::WRITE;
    pub const RX: u64 = Self::READ | Self::EXEC;
    pub const RWX: u64 = Self::READ | Self::WRITE | Self::EXEC;

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn new(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn bits(&self) -> u64 {
        self.0
    }

    #[inline]
    pub fn contains(&self, flag: u64) -> bool {
        (self.0 & flag) == flag
    }

    #[inline]
    pub fn set(&mut self, flag: u64) {
        self.0 |= flag;
    }

    #[inline]
    pub fn clear(&mut self, flag: u64) {
        self.0 &= !flag;
    }

    /// 是否可读
    pub fn is_read(&self) -> bool {
        self.contains(Self::READ)
    }

    /// 是否可写
    pub fn is_write(&self) -> bool {
        self.contains(Self::WRITE)
    }

    /// 是否可执行
    pub fn is_exec(&self) -> bool {
        self.contains(Self::EXEC)
    }

    /// 是否匿名映射
    pub fn is_anonymous(&self) -> bool {
        self.contains(Self::ANONYMOUS)
    }

    /// 是否共享
    pub fn is_shared(&self) -> bool {
        self.contains(Self::SHARED)
    }

    /// 转换为页表标志
    pub fn to_pte_flags(&self) -> u64 {
        let mut pte: u64 = 1; // Present

        if self.is_write() {
            pte |= 1 << 1; // Writable
        }
        if !self.is_exec() {
            pte |= 1 << 63; // NX bit
        }
        // User accessible
        pte |= 1 << 2;

        pte
    }
}

// ============================================================================
// VMA 信息 (存储在 MapleTree 中)
// ============================================================================

/// VMA 信息
///
/// 存储在 MapleTree 中，区间 [start, end) 作为 MapleTree 的键。
#[derive(Clone)]
pub struct VmaInfo {
    /// 权限和属性标志
    pub flags: VmFlags,
    /// 文件偏移 (如果是文件映射)
    pub pgoff: u64,
    /// 关联的文件 (如果是文件映射，暂为空)
    pub file: *mut (),
    /// 私有数据
    pub private_data: *mut (),
}

// VmaInfo 中的裸指针需要 Send/Sync
unsafe impl Send for VmaInfo {}
unsafe impl Sync for VmaInfo {}

impl VmaInfo {
    pub fn new(flags: VmFlags) -> Self {
        Self {
            flags,
            pgoff: 0,
            file: core::ptr::null_mut(),
            private_data: core::ptr::null_mut(),
        }
    }
}

// ============================================================================
// VMA 便利包装 (用于查询结果)
// ============================================================================

/// 虚拟内存区域 (查询结果包装)
pub struct Vma {
    /// 起始虚拟地址 (页对齐)
    pub vm_start: u64,
    /// 结束虚拟地址 (不包含，页对齐)
    pub vm_end: u64,
    /// 权限和属性标志
    pub vm_flags: VmFlags,
    /// 文件偏移
    pub vm_pgoff: u64,
}

impl Vma {
    /// 从 MapleTree 查询结果构造
    pub fn from_tree(start: usize, end: usize, info: &VmaInfo) -> Self {
        Self {
            vm_start: start as u64,
            vm_end: end as u64,
            vm_flags: info.flags,
            vm_pgoff: info.pgoff,
        }
    }

    /// 区域大小
    #[inline]
    pub fn size(&self) -> u64 {
        self.vm_end - self.vm_start
    }

    /// 页数
    #[inline]
    pub fn nr_pages(&self) -> u64 {
        self.size() / PAGE_SIZE
    }

    /// 检查地址是否在此 VMA 内
    #[inline]
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.vm_start && addr < self.vm_end
    }

    /// 检查范围是否与此 VMA 重叠
    #[inline]
    pub fn overlaps(&self, start: u64, end: u64) -> bool {
        self.vm_start < end && start < self.vm_end
    }
}

// ============================================================================
// 地址空间 (Mm)
// ============================================================================

/// 进程地址空间
///
/// 管理一个进程的所有虚拟内存区域
pub struct Mm {
    /// VMA 树 (区间 -> VmaInfo)
    pub vma_tree: MapleTree<VmaInfo>,
    /// VMA 数量
    pub vma_count: u32,
    /// 引用计数
    pub mm_count: u32,
    /// 用户数量 (共享此 mm 的线程数)
    pub mm_users: u32,
    /// PML4 页表物理地址
    pub pgd: u64,

    // ========== 地址空间布局 ==========
    /// 代码段起始
    pub start_code: u64,
    /// 代码段结束
    pub end_code: u64,
    /// 数据段起始
    pub start_data: u64,
    /// 数据段结束
    pub end_data: u64,
    /// 堆起始 (brk 基址)
    pub start_brk: u64,
    /// 当前 brk 位置
    pub brk: u64,
    /// 栈起始 (栈顶)
    pub start_stack: u64,
    /// 参数起始
    pub arg_start: u64,
    /// 参数结束
    pub arg_end: u64,
    /// 环境变量起始
    pub env_start: u64,
    /// 环境变量结束
    pub env_end: u64,

    // ========== mmap 区域 ==========
    /// mmap 区域起始
    pub mmap_base: u64,
    /// mmap 区域当前位置 (向下增长)
    pub mmap_legacy_base: u64,

    // ========== 统计信息 ==========
    /// 已映射页数
    pub total_vm: u64,
    /// 锁定页数
    pub locked_vm: u64,
    /// 共享页数
    pub shared_vm: u64,
    /// 可执行页数
    pub exec_vm: u64,
    /// 栈页数
    pub stack_vm: u64,
    /// 数据页数
    pub data_vm: u64,
}

impl Mm {
    /// 创建未初始化的 Mm
    pub fn uninit() -> Self {
        Self {
            vma_tree: MapleTree::new(),
            vma_count: 0,
            mm_count: 1,
            mm_users: 1,
            pgd: 0,
            start_code: 0,
            end_code: 0,
            start_data: 0,
            end_data: 0,
            start_brk: 0,
            brk: 0,
            start_stack: 0,
            arg_start: 0,
            arg_end: 0,
            env_start: 0,
            env_end: 0,
            mmap_base: 0,
            mmap_legacy_base: 0,
            total_vm: 0,
            locked_vm: 0,
            shared_vm: 0,
            exec_vm: 0,
            stack_vm: 0,
            data_vm: 0,
        }
    }

    /// 初始化 Mm
    pub fn init(&mut self, pgd: u64) {
        self.vma_tree = MapleTree::new();
        self.vma_count = 0;
        self.mm_count = 1;
        self.mm_users = 1;
        self.pgd = pgd;

        // 设置默认地址布局 (用户空间: 0 - 0x7FFFFFFFFFFF)
        // mmap 区域从高地址向下增长
        self.mmap_base = 0x7FFF_F000_0000;
        self.mmap_legacy_base = self.mmap_base;
    }

    /// 查找包含指定地址的 VMA
    pub fn find_vma(&self, addr: u64) -> Option<Vma> {
        self.vma_tree.find(addr as usize).map(|(s, e, info)| {
            Vma::from_tree(s, e, info)
        })
    }

    /// 查找包含指定地址的 VMA 的标志
    pub fn find_vma_flags(&self, addr: u64) -> Option<VmFlags> {
        self.vma_tree.find(addr as usize).map(|(_, _, info)| info.flags)
    }

    /// 查找与指定范围重叠的 VMA
    pub fn find_vma_intersection(&self, start: u64, end: u64) -> Option<Vma> {
        self.vma_tree
            .iter_intersecting(start as usize, end as usize)
            .next()
            .map(|(s, e, info)| Vma::from_tree(s, e, info))
    }

    /// 在指定范围查找空闲区域
    ///
    /// 返回一个至少 size 大小的空闲区域起始地址
    pub fn find_free_area(&self, hint: u64, size: u64, _flags: VmFlags) -> Option<u64> {
        let size = page_align_up(size) as usize;
        let mut hint = page_align_up(hint) as usize;
        let limit = 0x7FFF_FFFF_F000usize; // 用户空间上限

        if hint == 0 {
            hint = PAGE_SIZE as usize; // 避免 NULL 指针区域
        }

        self.vma_tree
            .find_gap(size, hint, limit)
            .map(|addr| addr as u64)
    }

    /// 插入 VMA
    pub fn insert_vma(&mut self, start: u64, end: u64, info: VmaInfo) -> bool {
        let nr_pages = (end - start) / PAGE_SIZE;
        let flags = info.flags;

        if self.vma_tree.insert(start as usize, end as usize, info).is_err() {
            return false;
        }

        self.vma_count += 1;
        self.total_vm += nr_pages;

        // 更新统计
        if flags.contains(VmFlags::EXEC) {
            self.exec_vm += nr_pages;
        }
        if flags.contains(VmFlags::GROWSDOWN) {
            self.stack_vm += nr_pages;
        }

        true
    }

    /// 从树中移除 VMA
    pub fn remove_vma(&mut self, start: u64) -> Option<(u64, VmaInfo)> {
        let (end, info) = self.vma_tree.remove(start as usize)?;
        let end = end as u64;
        let nr_pages = (end - start) / PAGE_SIZE;

        self.vma_count -= 1;
        self.total_vm -= nr_pages;

        if info.flags.contains(VmFlags::EXEC) {
            self.exec_vm -= nr_pages;
        }
        if info.flags.contains(VmFlags::GROWSDOWN) {
            self.stack_vm -= nr_pages;
        }

        Some((end, info))
    }

    /// brk 系统调用实现
    ///
    /// 扩展或收缩堆
    pub fn do_brk(&mut self, new_brk: u64) -> Result<u64, &'static str> {
        let new_brk = page_align_up(new_brk);
        let old_brk = self.brk;

        if new_brk < self.start_brk {
            return Err("brk below start");
        }

        if new_brk == old_brk {
            return Ok(old_brk);
        }

        if new_brk > old_brk {
            // 扩展堆
            // 检查是否与其他 VMA 冲突
            if self.find_vma_intersection(old_brk, new_brk).is_some() {
                return Err("brk conflicts with existing VMA");
            }

            // 查找堆 VMA 并扩展
            let heap_start = old_brk.saturating_sub(1);
            if let Some((s, e, info)) = self.vma_tree.find(heap_start as usize) {
                if info.flags.contains(VmFlags::HEAP) {
                    let start = s as u64;
                    let old_end = e as u64;
                    let delta = (new_brk - old_end) / PAGE_SIZE;
                    let new_info = info.clone();
                    // 使用 replace 原子替换，避免 remove+insert 导致 VMA 丢失
                    match self.vma_tree.replace(start as usize, new_brk as usize, new_info) {
                        Ok(_) => { self.total_vm += delta; }
                        Err(_) => { return Err("brk expansion failed"); }
                    }
                }
            }
        } else {
            // 收缩堆
            let heap_addr = new_brk;
            if let Some((s, _e, info)) = self.vma_tree.find(heap_addr as usize) {
                let start = s as u64;
                if info.flags.contains(VmFlags::HEAP) && start <= new_brk {
                    let delta = (old_brk - new_brk) / PAGE_SIZE;
                    let new_info = info.clone();
                    match self.vma_tree.replace(start as usize, new_brk as usize, new_info) {
                        Ok(_) => { self.total_vm -= delta; }
                        Err(_) => { return Err("brk shrink failed"); }
                    }
                }
            }
        }

        self.brk = new_brk;
        Ok(self.brk)
    }

    /// 扩展栈 VMA (向下增长)
    pub fn expand_stack(&mut self, vma_start: u64, new_start: u64) -> bool {
        let old_start = vma_start;
        // 预检查：确保 [new_start, old_start) 区间没有其他 VMA
        if self.find_vma_intersection(new_start, old_start).is_some() {
            return false;
        }
        if let Some((end, info)) = self.vma_tree.remove(old_start as usize) {
            let info_backup = info.clone();
            if self.vma_tree.insert(new_start as usize, end, info).is_ok() {
                let delta = (old_start - new_start) / PAGE_SIZE;
                self.stack_vm += delta;
                self.total_vm += delta;
                return true;
            }
            // insert 失败，恢复原 VMA 防止丢失
            let _ = self.vma_tree.insert(old_start as usize, end, info_backup);
        }
        false
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 页对齐 (向上)
#[inline]
const fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

// ============================================================================
// mmap 相关
// ============================================================================

/// mmap 标志 (与 Linux 兼容)
pub mod mmap_flags {
    pub const MAP_SHARED: u32     = 0x01;
    pub const MAP_PRIVATE: u32    = 0x02;
    pub const MAP_FIXED: u32      = 0x10;
    pub const MAP_ANONYMOUS: u32  = 0x20;
    pub const MAP_GROWSDOWN: u32  = 0x0100;
    pub const MAP_LOCKED: u32     = 0x2000;
    pub const MAP_HUGETLB: u32    = 0x40000;
}

/// mmap 保护标志 (与 Linux 兼容)
pub mod prot_flags {
    pub const PROT_NONE: u32  = 0x0;
    pub const PROT_READ: u32  = 0x1;
    pub const PROT_WRITE: u32 = 0x2;
    pub const PROT_EXEC: u32  = 0x4;
}

/// 将 mmap 标志转换为 VmFlags
pub fn mmap_flags_to_vm_flags(prot: u32, flags: u32) -> VmFlags {
    let mut vm_flags = VmFlags::empty();

    if prot & prot_flags::PROT_READ != 0 {
        vm_flags.set(VmFlags::READ);
    }
    if prot & prot_flags::PROT_WRITE != 0 {
        vm_flags.set(VmFlags::WRITE);
        vm_flags.set(VmFlags::MAYWRITE);
    }
    if prot & prot_flags::PROT_EXEC != 0 {
        vm_flags.set(VmFlags::EXEC);
    }

    if flags & mmap_flags::MAP_SHARED != 0 {
        vm_flags.set(VmFlags::SHARED);
    }
    if flags & mmap_flags::MAP_ANONYMOUS != 0 {
        vm_flags.set(VmFlags::ANONYMOUS);
    }
    if flags & mmap_flags::MAP_GROWSDOWN != 0 {
        vm_flags.set(VmFlags::GROWSDOWN);
    }
    if flags & mmap_flags::MAP_LOCKED != 0 {
        vm_flags.set(VmFlags::LOCKED);
    }
    if flags & mmap_flags::MAP_HUGETLB != 0 {
        vm_flags.set(VmFlags::HUGETLB);
    }

    vm_flags
}

// ============================================================================
// 初始化
// ============================================================================

/// 内核 mm (共享内核页表)
static INIT_MM: crate::sync::OnceCell<SpinLock<Mm>> = crate::sync::OnceCell::new();

/// 获取内核 mm (加锁)
pub fn get_init_mm() -> crate::sync::SpinLockGuard<'static, Mm> {
    INIT_MM.get_or_init(|| SpinLock::new(Mm::uninit())).lock()
}

/// 初始化 VMA 子系统
pub fn init_vma() {
    let mut mm = INIT_MM.get_or_init(|| SpinLock::new(Mm::uninit())).lock();
    mm.init(kernel_pgd_phys());
}

#[inline]
fn kernel_pgd_phys() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::mm::arch::read_cr3() & crate::mm::arch::PTE_ADDR_MASK
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        0
    }
}
