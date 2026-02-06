// ============================================================================
// january_os - VMA (Virtual Memory Area) 虚拟内存区域管理
//
// 参考 Linux vm_area_struct，管理进程的虚拟地址空间
// ============================================================================

use core::ptr;
use super::layout::PAGE_SIZE;
use crate::mm::page::page::{Page, ListHead};

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
// VMA 结构
// ============================================================================

/// 虚拟内存区域
/// 
/// 表示进程地址空间中的一个连续区域
#[repr(C)]
pub struct Vma {
    /// 起始虚拟地址 (页对齐)
    pub vm_start: u64,
    /// 结束虚拟地址 (不包含，页对齐)
    pub vm_end: u64,
    /// 权限和属性标志
    pub vm_flags: VmFlags,
    /// 文件偏移 (如果是文件映射)
    pub vm_pgoff: u64,
    /// 所属地址空间
    pub vm_mm: *mut Mm,
    /// 链表节点 (用于 mm->vma_list)
    pub vm_list: ListHead,
    /// 关联的文件 (如果是文件映射，暂为空)
    pub vm_file: *mut (),
    /// 私有数据
    pub vm_private_data: *mut (),
}

impl Vma {
    /// 创建未初始化的 VMA
    pub const fn uninit() -> Self {
        Self {
            vm_start: 0,
            vm_end: 0,
            vm_flags: VmFlags::empty(),
            vm_pgoff: 0,
            vm_mm: ptr::null_mut(),
            vm_list: ListHead::new(),
            vm_file: ptr::null_mut(),
            vm_private_data: ptr::null_mut(),
        }
    }
    
    /// 初始化 VMA
    pub fn init(&mut self, start: u64, end: u64, flags: VmFlags) {
        self.vm_start = start;
        self.vm_end = end;
        self.vm_flags = flags;
        self.vm_pgoff = 0;
        self.vm_list.init();
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
    
    /// 是否可与另一个 VMA 合并
    pub fn can_merge(&self, other: &Vma) -> bool {
        // 标志相同
        if self.vm_flags.bits() != other.vm_flags.bits() {
            return false;
        }
        // 不能合并
        if self.vm_flags.contains(VmFlags::DONTMERGE) {
            return false;
        }
        // 地址相邻
        if self.vm_end != other.vm_start && other.vm_end != self.vm_start {
            return false;
        }
        // 文件映射需要偏移连续
        if !self.vm_file.is_null() {
            // 简化：暂不支持文件映射合并
            return false;
        }
        true
    }
}

// ============================================================================
// 地址空间 (Mm)
// ============================================================================

/// 进程地址空间
/// 
/// 管理一个进程的所有虚拟内存区域
#[repr(C)]
pub struct Mm {
    /// VMA 链表头
    pub vma_list: ListHead,
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
    pub const fn uninit() -> Self {
        Self {
            vma_list: ListHead::new(),
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
        self.vma_list.init();
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
    pub fn find_vma(&self, addr: u64) -> Option<&Vma> {
        unsafe {
            let mut node = self.vma_list.next;
            let head = &self.vma_list as *const _ as *mut ListHead;
            
            while node != head {
                let vma = container_of!(node, Vma, vm_list);
                if (*vma).contains(addr) {
                    return Some(&*vma);
                }
                // VMA 按地址排序，如果当前 VMA 起始地址已超过目标，可以提前退出
                if (*vma).vm_start > addr {
                    break;
                }
                node = (*node).next;
            }
            None
        }
    }
    
    /// 查找包含指定地址的 VMA (可变)
    pub fn find_vma_mut(&mut self, addr: u64) -> Option<&mut Vma> {
        unsafe {
            let mut node = self.vma_list.next;
            let head = &self.vma_list as *const _ as *mut ListHead;
            
            while node != head {
                let vma = container_of!(node, Vma, vm_list);
                if (*vma).contains(addr) {
                    return Some(&mut *vma);
                }
                if (*vma).vm_start > addr {
                    break;
                }
                node = (*node).next;
            }
            None
        }
    }
    
    /// 查找与指定范围重叠的 VMA
    pub fn find_vma_intersection(&self, start: u64, end: u64) -> Option<&Vma> {
        unsafe {
            let mut node = self.vma_list.next;
            let head = &self.vma_list as *const _ as *mut ListHead;
            
            while node != head {
                let vma = container_of!(node, Vma, vm_list);
                if (*vma).overlaps(start, end) {
                    return Some(&*vma);
                }
                if (*vma).vm_start >= end {
                    break;
                }
                node = (*node).next;
            }
            None
        }
    }
    
    /// 在指定范围查找空闲区域
    /// 
    /// 返回一个至少 size 大小的空闲区域起始地址
    pub fn find_free_area(&self, hint: u64, size: u64, flags: VmFlags) -> Option<u64> {
        let size = page_align_up(size);
        
        // 从 hint 开始向上搜索
        let mut addr = page_align_up(hint);
        let limit = 0x7FFF_FFFF_F000u64; // 用户空间上限
        
        if addr == 0 {
            addr = PAGE_SIZE; // 避免 NULL 指针区域
        }
        
        unsafe {
            let mut node = self.vma_list.next;
            let head = &self.vma_list as *const _ as *mut ListHead;
            
            // 跳过 hint 之前的 VMA
            while node != head {
                let vma = &*container_of!(node, Vma, vm_list);
                if vma.vm_end > addr {
                    break;
                }
                node = (*node).next;
            }
            
            // 查找空闲区域
            while node != head {
                let vma = &*container_of!(node, Vma, vm_list);
                
                // 检查当前位置到下一个 VMA 之间是否有足够空间
                if vma.vm_start >= addr + size {
                    // 找到空闲区域
                    if addr + size <= limit {
                        return Some(addr);
                    }
                }
                
                // 移动到当前 VMA 之后
                addr = page_align_up(vma.vm_end);
                node = (*node).next;
            }
            
            // 检查最后一个 VMA 之后的空间
            if addr + size <= limit {
                return Some(addr);
            }
        }
        
        None
    }
    
    /// 插入 VMA 到链表 (保持地址排序)
    /// 
    /// # Safety
    /// 
    /// vma 必须是有效指针，且不在任何链表中
    pub unsafe fn insert_vma(&mut self, vma: &mut Vma) {
        let mut node = self.vma_list.next;
        let head = &mut self.vma_list as *mut ListHead;
        
        // 找到插入位置
        while node != head {
            let curr = &*container_of!(node, Vma, vm_list);
            if curr.vm_start > vma.vm_start {
                break;
            }
            node = (*node).next;
        }
        
        // 在 node 之前插入
        vma.vm_list.next = node;
        vma.vm_list.prev = (*node).prev;
        (*(*node).prev).next = &mut vma.vm_list;
        (*node).prev = &mut vma.vm_list;
        
        vma.vm_mm = self;
        self.vma_count += 1;
        self.total_vm += vma.nr_pages();
        
        // 更新统计
        if vma.vm_flags.contains(VmFlags::EXEC) {
            self.exec_vm += vma.nr_pages();
        }
        if vma.vm_flags.contains(VmFlags::GROWSDOWN) {
            self.stack_vm += vma.nr_pages();
        }
    }
    
    /// 从链表移除 VMA
    pub unsafe fn remove_vma(&mut self, vma: &mut Vma) {
        vma.vm_list.del();
        self.vma_count -= 1;
        self.total_vm -= vma.nr_pages();
        
        if vma.vm_flags.contains(VmFlags::EXEC) {
            self.exec_vm -= vma.nr_pages();
        }
        if vma.vm_flags.contains(VmFlags::GROWSDOWN) {
            self.stack_vm -= vma.nr_pages();
        }
        
        vma.vm_mm = ptr::null_mut();
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
            
            // 更新堆 VMA
            let delta = (new_brk - old_brk) / PAGE_SIZE;
            unsafe {
                if let Some(vma_ptr) = self.find_heap_vma_ptr(old_brk.saturating_sub(1)) {
                    (*vma_ptr).vm_end = new_brk;
                    self.total_vm += delta;
                }
            }
        } else {
            // 收缩堆
            let delta = (old_brk - new_brk) / PAGE_SIZE;
            unsafe {
                if let Some(vma_ptr) = self.find_heap_vma_ptr(new_brk) {
                    if (*vma_ptr).vm_start <= new_brk {
                        self.total_vm -= delta;
                        (*vma_ptr).vm_end = new_brk;
                    }
                }
            }
        }
        
        self.brk = new_brk;
        Ok(self.brk)
    }
    
    /// 查找堆 VMA 指针 (避免借用冲突)
    unsafe fn find_heap_vma_ptr(&self, addr: u64) -> Option<*mut Vma> {
        let mut node = self.vma_list.next;
        let head = &self.vma_list as *const _ as *mut ListHead;
        
        while node != head {
            let vma = container_of!(node, Vma, vm_list);
            if (*vma).contains(addr) && (*vma).vm_flags.contains(VmFlags::HEAP) {
                return Some(vma);
            }
            if (*vma).vm_start > addr {
                break;
            }
            node = (*node).next;
        }
        None
    }
}

// ============================================================================
// 辅助函数和宏
// ============================================================================

/// 页对齐 (向上)
#[inline]
const fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

/// container_of 宏
macro_rules! container_of {
    ($ptr:expr, $type:ty, $field:ident) => {{
        let ptr = $ptr as *const u8;
        let offset = core::mem::offset_of!($type, $field);
        unsafe { ptr.sub(offset) as *mut $type }
    }};
}

pub(crate) use container_of;

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
static mut INIT_MM: Mm = Mm::uninit();

/// 获取内核 mm
pub fn get_init_mm() -> &'static mut Mm {
    unsafe { &mut *core::ptr::addr_of_mut!(INIT_MM) }
}

/// 初始化 VMA 子系统
pub fn init_vma() {
    unsafe {
        (*core::ptr::addr_of_mut!(INIT_MM)).init(0); // 内核 pgd 后续设置
    }
}
