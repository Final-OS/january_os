// ============================================================================
// january_os - VMA (Virtual Memory Area) 虚拟内存区域管理
//
// 参考 Linux vm_area_struct，管理进程的虚拟地址空间
// 使用 MapleTree 进行 O(log n) 的区间查找和间隙搜索
// ============================================================================

use super::layout::{
    PAGE_SIZE,
    USER_MMAP_BASE,
    USER_SPACE_END,
    USER_SPACE_START,
    USER_STACK_SIZE,
    USER_STACK_TOP,
};
use alloc::boxed::Box;
use crate::libs::mptree::MapleTree;
use crate::sync::IrqSpinLock;
use core::sync::atomic::{AtomicU32, Ordering};

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

    /// 转换为用户页表标志
    ///
    /// 仅用于用户空间 VMA。内核映射不得复用该接口（该接口固定带 `PTE_USER`）。
    pub fn to_user_pte_flags(&self) -> u64 {
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
    /// 文件数据基址（最小静态文件后端）
    pub vm_file: *mut (),
    /// 文件数据长度（字节，编码在指针宽度中）
    pub vm_private_data: *mut (),
}

impl Vma {
    /// 从 MapleTree 查询结果构造
    pub fn from_tree(start: usize, end: usize, info: &VmaInfo) -> Self {
        Self {
            vm_start: start as u64,
            vm_end: end as u64,
            vm_flags: info.flags,
            vm_pgoff: info.pgoff,
            vm_file: info.file,
            vm_private_data: info.private_data,
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
    /// 地址空间内部锁（保护 VMA 树和统计字段）
    pub lock: IrqSpinLock<()>,
    /// VMA 树 (区间 -> VmaInfo)
    pub vma_tree: MapleTree<VmaInfo>,
    /// VMA 数量
    pub vma_count: u32,
    /// 引用计数
    pub mm_count: AtomicU32,
    /// 用户数量 (共享此 mm 的线程数)
    pub mm_users: AtomicU32,
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
    #[inline]
    fn stack_top_addr(&self) -> u64 {
        if self.start_stack != 0 {
            self.start_stack
        } else {
            USER_STACK_TOP
        }
    }

    #[inline]
    fn stack_expand_min_addr(&self) -> u64 {
        self.stack_top_addr().saturating_sub(USER_STACK_SIZE)
    }

    /// 创建未初始化的 Mm
    pub fn uninit() -> Self {
        Self {
            lock: IrqSpinLock::new(()),
            vma_tree: MapleTree::new(),
            vma_count: 0,
            mm_count: AtomicU32::new(1),
            mm_users: AtomicU32::new(1),
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
        self.mm_count.store(1, Ordering::Relaxed);
        self.mm_users.store(1, Ordering::Relaxed);
        self.pgd = pgd;

        // 设置默认地址布局 (用户空间: 0 - 0x7FFFFFFFFFFF)
        // mmap 区域从高地址向下增长
        self.mmap_base = USER_MMAP_BASE;
        self.mmap_legacy_base = self.mmap_base;
    }

    /// 查找包含指定地址的 VMA
    pub fn find_vma(&self, addr: u64) -> Option<Vma> {
        let _guard = self.lock.lock();
        self.find_vma_nolock(addr)
    }

    #[inline]
    fn find_vma_nolock(&self, addr: u64) -> Option<Vma> {
        self.vma_tree.find(addr as usize).map(|(s, e, info)| {
            Vma::from_tree(s, e, info)
        })
    }

    /// 查找包含指定地址的 VMA 的标志
    pub fn find_vma_flags(&self, addr: u64) -> Option<VmFlags> {
        let _guard = self.lock.lock();
        self.find_vma_flags_nolock(addr)
    }

    #[inline]
    fn find_vma_flags_nolock(&self, addr: u64) -> Option<VmFlags> {
        self.vma_tree.find(addr as usize).map(|(_, _, info)| info.flags)
    }

    /// 查找与指定范围重叠的 VMA
    pub fn find_vma_intersection(&self, start: u64, end: u64) -> Option<Vma> {
        let _guard = self.lock.lock();
        self.find_vma_intersection_nolock(start, end)
    }

    #[inline]
    fn find_vma_intersection_nolock(&self, start: u64, end: u64) -> Option<Vma> {
        self.vma_tree
            .iter_intersecting(start as usize, end as usize)
            .next()
            .map(|(s, e, info)| Vma::from_tree(s, e, info))
    }

    /// 在指定范围查找空闲区域
    ///
    /// 返回一个至少 size 大小的空闲区域起始地址
    pub fn find_free_area(&self, hint: u64, size: u64, _flags: VmFlags) -> Option<u64> {
        let _guard = self.lock.lock();
        self.find_free_area_nolock(hint, size)
    }

    fn find_free_area_nolock(&self, hint: u64, size: u64) -> Option<u64> {
        let size = page_align_up(size) as usize;
        let mut hint = page_align_up(hint) as usize;
        let user_start = USER_SPACE_START as usize;
        let limit = USER_SPACE_END.saturating_sub(PAGE_SIZE) as usize; // 用户空间上限（最后一页起始）

        if hint == 0 || hint < user_start {
            hint = user_start;
        }

        self.vma_tree
            .find_gap(size, hint, limit)
            .map(|addr| addr as u64)
    }

    /// 插入 VMA
    pub fn insert_vma(&mut self, start: u64, end: u64, info: VmaInfo) -> bool {
        let _guard = unsafe { (*core::ptr::addr_of!(self.lock)).lock() };
        let mm = self as *mut Self;
        unsafe { (*mm).insert_vma_nolock(start, end, info) }
    }

    fn insert_vma_nolock(&mut self, start: u64, end: u64, info: VmaInfo) -> bool {
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
        let _guard = unsafe { (*core::ptr::addr_of!(self.lock)).lock() };
        let mm = self as *mut Self;
        unsafe { (*mm).remove_vma_nolock(start) }
    }

    fn remove_vma_nolock(&mut self, start: u64) -> Option<(u64, VmaInfo)> {
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
        let _guard = unsafe { (*core::ptr::addr_of!(self.lock)).lock() };
        let mm = self as *mut Self;
        unsafe { (*mm).do_brk_nolock(new_brk) }
    }

    fn do_brk_nolock(&mut self, new_brk: u64) -> Result<u64, &'static str> {
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
            if self.find_vma_intersection_nolock(old_brk, new_brk).is_some() {
                return Err("brk conflicts with existing VMA");
            }

            // 查找堆 VMA 并扩展
            let heap_start = old_brk.saturating_sub(1);
            let mut expanded_existing_heap = false;
            if let Some((s, e, info)) = self.vma_tree.find(heap_start as usize) {
                if info.flags.contains(VmFlags::HEAP) {
                    let start = s as u64;
                    let old_end = e as u64;
                    let delta = new_brk.saturating_sub(old_end) / PAGE_SIZE;
                    let new_info = info.clone();
                    // 使用 replace 原子替换，避免 remove+insert 导致 VMA 丢失
                    match self.vma_tree.replace(start as usize, new_brk as usize, new_info) {
                        Ok(_) => {
                            self.total_vm += delta;
                            expanded_existing_heap = true;
                        }
                        Err(_) => return Err("brk expansion failed"),
                    }
                }
            }
            if !expanded_existing_heap {
                let mut heap_flags = VmFlags::empty();
                heap_flags.set(VmFlags::READ);
                heap_flags.set(VmFlags::WRITE);
                heap_flags.set(VmFlags::MAYWRITE);
                heap_flags.set(VmFlags::ANONYMOUS);
                heap_flags.set(VmFlags::HEAP);
                if !self.insert_vma_nolock(old_brk, new_brk, VmaInfo::new(heap_flags)) {
                    return Err("brk create heap vma failed");
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
        let _guard = unsafe { (*core::ptr::addr_of!(self.lock)).lock() };
        let mm = self as *mut Self;
        unsafe { (*mm).expand_stack_nolock(vma_start, new_start) }
    }

    fn expand_stack_nolock(&mut self, vma_start: u64, new_start: u64) -> bool {
        let old_start = vma_start;
        if new_start >= old_start {
            return false;
        }
        if self
            .vma_tree
            .move_start(old_start as usize, new_start as usize)
            .is_ok()
        {
            let delta = (old_start - new_start) / PAGE_SIZE;
            self.stack_vm += delta;
            self.total_vm += delta;
            return true;
        }
        false
    }

    /// 缺页路径使用：在用户栈增长限制内扩展栈并返回更新后的 VMA。
    pub fn expand_stack_for_fault(&mut self, fault_addr: u64) -> Option<Vma> {
        let _guard = unsafe { (*core::ptr::addr_of!(self.lock)).lock() };
        let mm = self as *mut Self;
        unsafe { (*mm).expand_stack_for_fault_nolock(fault_addr) }
    }

    fn expand_stack_for_fault_nolock(&mut self, fault_addr: u64) -> Option<Vma> {
        let page_addr = fault_addr & !(PAGE_SIZE - 1);
        let stack_top = self.stack_top_addr();
        let stack_min = self.stack_expand_min_addr();
        let stack_bottom = stack_top.saturating_sub(self.stack_vm * PAGE_SIZE);

        if page_addr < stack_min || page_addr >= stack_bottom {
            return None;
        }

        let stack_info = self.vma_tree.lower_bound(page_addr as usize);
        match stack_info {
            Some((s, _e, info)) if info.flags.contains(VmFlags::GROWSDOWN)
                && (s as u64) > page_addr =>
            {
                let vma_start = s as u64;
                if !self.expand_stack_nolock(vma_start, page_addr) {
                    return None;
                }
                self.find_vma_nolock(page_addr)
            }
            _ => None,
        }
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
static INIT_MM: crate::sync::OnceCell<IrqSpinLock<Mm>> = crate::sync::OnceCell::new();

/// 获取内核 mm (加锁)
pub fn get_init_mm() -> crate::sync::IrqSpinLockGuard<'static, Mm> {
    INIT_MM.get_or_init(|| IrqSpinLock::new(Mm::uninit())).lock()
}

/// 获取内核初始化地址空间的裸指针
///
/// 该指针在整个内核生命周期内稳定有效。
/// 调用方必须遵守 Mm 内部锁约束。
pub fn init_mm_ptr() -> *mut Mm {
    let mut mm = get_init_mm();
    &mut *mm as *mut Mm
}

/// 引用一个现有地址空间（用于 CLONE_VM 等共享语义）。
pub fn mm_retain(mm: *mut Mm) -> *mut Mm {
    let init_ptr = init_mm_ptr();
    let target = if mm.is_null() { init_ptr } else { mm };

    if target == init_ptr {
        return target;
    }

    unsafe {
        (*target).mm_count.fetch_add(1, Ordering::AcqRel);
        (*target).mm_users.fetch_add(1, Ordering::AcqRel);
    }
    target
}

/// 克隆地址空间元数据（VMA/布局统计），用于 fork 私有 mm。
///
/// 当前阶段页表 `pgd` 仍复用父进程值；后续可在此接入真正页表复制。
pub fn mm_clone(mm: *mut Mm) -> *mut Mm {
    let init_ptr = init_mm_ptr();
    let src_ptr = if mm.is_null() { init_ptr } else { mm };
    let src = unsafe { &mut *src_ptr };
    let _src_guard = src.lock.lock();

    let mut dst = Mm::uninit();
    dst.init(src.pgd);

    dst.start_code = src.start_code;
    dst.end_code = src.end_code;
    dst.start_data = src.start_data;
    dst.end_data = src.end_data;
    dst.start_brk = src.start_brk;
    dst.brk = src.brk;
    dst.start_stack = src.start_stack;
    dst.arg_start = src.arg_start;
    dst.arg_end = src.arg_end;
    dst.env_start = src.env_start;
    dst.env_end = src.env_end;
    dst.mmap_base = src.mmap_base;
    dst.mmap_legacy_base = src.mmap_legacy_base;
    dst.total_vm = src.total_vm;
    dst.locked_vm = src.locked_vm;
    dst.shared_vm = src.shared_vm;
    dst.exec_vm = src.exec_vm;
    dst.stack_vm = src.stack_vm;
    dst.data_vm = src.data_vm;

    let mut inserted = 0u32;
    for (start, end, info) in src.vma_tree.iter() {
        if dst.vma_tree.insert(start, end, info.clone()).is_ok() {
            inserted = inserted.saturating_add(1);
        }
    }
    dst.vma_count = inserted;

    Box::into_raw(Box::new(dst))
}

/// 释放地址空间引用；引用计数归零时回收 mm 对象。
///
/// `init_mm` 为全局常驻对象，不参与释放。
pub unsafe fn mm_release(mm: *mut Mm) {
    if mm.is_null() {
        return;
    }

    let init_ptr = init_mm_ptr();
    if mm == init_ptr {
        return;
    }

    let prev = (*mm).mm_count.fetch_sub(1, Ordering::AcqRel);
    let _ = (*mm).mm_users.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
        if v > 0 {
            Some(v - 1)
        } else {
            Some(0)
        }
    });

    if prev <= 1 {
        drop(Box::from_raw(mm));
    }
}

/// 初始化 VMA 子系统
pub fn init_vma() {
    let mut mm = INIT_MM.get_or_init(|| IrqSpinLock::new(Mm::uninit())).lock();
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
