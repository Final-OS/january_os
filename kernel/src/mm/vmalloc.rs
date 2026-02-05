// ============================================================================
// january_os - vmalloc 虚拟连续内存分配
//
// 分配虚拟地址连续但物理地址不连续的内存
// 用于大块内核内存分配（如模块、大缓冲区等）
// ============================================================================

use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};
use core::panic::Location;
use crate::sync::OnceCell;
use super::layout::PAGE_SIZE;
use super::page::{Page, ListHead, pfn_to_page, page_to_pfn};
use super::zone::GfpFlags;
use super::buddy::{alloc_page, free_page};
use super::paging::{PageTableManager, PTE_PRESENT, PTE_WRITABLE, PTE_GLOBAL, PTE_NO_CACHE};

// ============================================================================
// 常量
// ============================================================================

/// vmalloc 区域起始地址 (内核高地址空间)
pub const VMALLOC_START: u64 = 0xFFFF_C900_0000_0000;

/// vmalloc 区域结束地址
pub const VMALLOC_END: u64   = 0xFFFF_E8FF_FFFF_FFFF;

/// vmalloc 区域大小
pub const VMALLOC_SIZE: u64  = VMALLOC_END - VMALLOC_START;

/// 最大 vmalloc 区域数
const MAX_VMALLOC_AREAS: usize = 256;

// ============================================================================
// vmalloc 区域描述
// ============================================================================

/// vmalloc 区域标志
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct VmFlags(u32);

impl VmFlags {
    /// 已分配
    pub const ALLOC: u32    = 1 << 0;
    /// 正在使用
    pub const INUSE: u32    = 1 << 1;
    /// 用户映射
    pub const USERMAP: u32  = 1 << 2;
    /// IO 映射
    pub const IOREMAP: u32  = 1 << 3;
    
    pub const fn empty() -> Self {
        Self(0)
    }
    
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }
    
    pub fn contains(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
    
    pub fn set(&mut self, flag: u32) {
        self.0 |= flag;
    }
    
    pub fn clear(&mut self, flag: u32) {
        self.0 &= !flag;
    }
}

/// vmalloc 区域
#[repr(C)]
pub struct VmStruct {
    /// 起始虚拟地址
    pub addr: u64,
    /// 大小 (字节)
    pub size: u64,
    /// 标志
    pub flags: VmFlags,
    /// 物理页数组
    pub pages: *mut *mut Page,
    /// 页数
    pub nr_pages: u32,
    /// 填充
    _pad: u32,
    /// 链表节点
    pub list: ListHead,
    /// 调用者信息 (调试用)
    pub caller: Option<&'static Location<'static>>,
}

impl VmStruct {
    pub const fn uninit() -> Self {
        Self {
            addr: 0,
            size: 0,
            flags: VmFlags::empty(),
            pages: ptr::null_mut(),
            nr_pages: 0,
            _pad: 0,
            list: ListHead::new(),
            caller: None,
        }
    }
    
    pub fn init(&mut self, addr: u64, size: u64, caller: Option<&'static Location<'static>>) {
        self.addr = addr;
        self.size = size;
        self.flags = VmFlags::new(VmFlags::ALLOC | VmFlags::INUSE);
        self.list.init();
        self.caller = caller;
    }
}

// ============================================================================
// 全局状态
// ============================================================================

/// vmalloc 区域数组
static mut VMALLOC_AREAS: [VmStruct; MAX_VMALLOC_AREAS] = {
    const UNINIT: VmStruct = VmStruct::uninit();
    [UNINIT; MAX_VMALLOC_AREAS]
};

/// 已使用的区域数
static VMALLOC_AREA_COUNT: AtomicU64 = AtomicU64::new(0);

/// 下一个空闲地址
static VMALLOC_NEXT_FREE: AtomicU64 = AtomicU64::new(VMALLOC_START);

/// vmalloc 状态
struct VmallocState {
    /// 直接映射偏移
    direct_map: u64,
    /// 页表管理器指针
    page_table_mgr: *mut PageTableManager,
}

// 允许跨线程发送（实际上是单核初始化）
unsafe impl Send for VmallocState {}
unsafe impl Sync for VmallocState {}

/// vmalloc 全局状态（一次性初始化）
static VMALLOC_STATE: OnceCell<VmallocState> = OnceCell::new();

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 vmalloc 子系统
pub unsafe fn init_vmalloc(direct_map: u64, pt_mgr: *mut PageTableManager) {
    VMALLOC_NEXT_FREE.store(VMALLOC_START, Ordering::SeqCst);
    let _ = VMALLOC_STATE.set(VmallocState {
        direct_map,
        page_table_mgr: pt_mgr,
    });
}

/// 检查是否已初始化
pub fn vmalloc_initialized() -> bool {
    VMALLOC_STATE.is_initialized()
}

// ============================================================================
// 核心分配函数
// ============================================================================

/// 分配虚拟连续内存
/// 
/// # Arguments
/// * `size` - 请求大小 (字节)
/// 
/// # Returns
/// 成功返回虚拟地址，失败返回 null
#[track_caller]
pub fn vmalloc(size: usize) -> *mut u8 {
    if size == 0 || !vmalloc_initialized() {
        return ptr::null_mut();
    }
    
    __vmalloc(size, GfpFlags::new(GfpFlags::KERNEL))
}

/// 分配并清零的虚拟连续内存
#[track_caller]
pub fn vzalloc(size: usize) -> *mut u8 {
    let ptr = vmalloc(size);
    if !ptr.is_null() {
        unsafe {
            ptr::write_bytes(ptr, 0, size);
        }
    }
    ptr
}

/// 释放 vmalloc 分配的内存
pub fn vfree(addr: *mut u8) {
    if addr.is_null() || !vmalloc_initialized() {
        return;
    }
    
    let addr = addr as u64;
    
    // 查找对应的 VmStruct
    unsafe {
        for i in 0..MAX_VMALLOC_AREAS {
            let area = &mut VMALLOC_AREAS[i];
            if area.flags.contains(VmFlags::INUSE) && area.addr == addr {
                __vfree(area);
                return;
            }
        }
    }
}

/// 内部分配函数
#[track_caller]
fn __vmalloc(size: usize, gfp: GfpFlags) -> *mut u8 {
    // 计算需要的页数
    let size = page_align_up(size as u64);
    let nr_pages = (size / PAGE_SIZE) as u32;
    
    unsafe {
        // 找一个空闲的 VmStruct
        let mut area: Option<&mut VmStruct> = None;
        for i in 0..MAX_VMALLOC_AREAS {
            if !VMALLOC_AREAS[i].flags.contains(VmFlags::ALLOC) {
                area = Some(&mut VMALLOC_AREAS[i]);
                break;
            }
        }
        
        let area = match area {
            Some(a) => a,
            None => return ptr::null_mut(),
        };
        
        // 分配虚拟地址空间
        let vaddr = allocate_vm_area(size);
        if vaddr == 0 {
            return ptr::null_mut();
        }
        
        // 初始化 VmStruct
        area.init(vaddr, size, Some(Location::caller()));
        area.nr_pages = nr_pages;
        
        // 分配物理页并映射
        // 简化实现：逐页分配和映射
        for i in 0..nr_pages {
            let page = match alloc_page(gfp) {
                Some(p) => p,
                None => {
                    // 分配失败，释放已分配的页
                    __vfree(area);
                    return ptr::null_mut();
                }
            };
            
            let phys = page_to_pfn(page) * PAGE_SIZE;
            let virt = vaddr + (i as u64) * PAGE_SIZE;
            
            // 映射页面
            if !map_vmalloc_page(virt, phys, PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL) {
                // 映射失败
                free_page(page);
                __vfree(area);
                return ptr::null_mut();
            }
        }
        
        VMALLOC_AREA_COUNT.fetch_add(1, Ordering::Relaxed);
        
        vaddr as *mut u8
    }
}

/// 内部释放函数
unsafe fn __vfree(area: &mut VmStruct) {
    if !area.flags.contains(VmFlags::INUSE) {
        return;
    }
    
    // 取消映射并释放物理页
    for i in 0..area.nr_pages {
        let virt = area.addr + (i as u64) * PAGE_SIZE;
        
        // 获取物理地址
        if let Some(phys) = get_vmalloc_phys(virt) {
            // 取消映射
            unmap_vmalloc_page(virt);
            
            // 释放物理页 (仅当不是 IO 映射时)
            if !area.flags.contains(VmFlags::IOREMAP) {
                let pfn = phys / PAGE_SIZE;
                let page = pfn_to_page(pfn);
                free_page(page);
            }
        }
    }
    
    // 清理 VmStruct
    area.flags = VmFlags::empty();
    area.addr = 0;
    area.size = 0;
    area.nr_pages = 0;
    
    VMALLOC_AREA_COUNT.fetch_sub(1, Ordering::Relaxed);
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 分配虚拟地址空间
fn allocate_vm_area(size: u64) -> u64 {
    // 简单实现：线性分配
    let addr = VMALLOC_NEXT_FREE.fetch_add(size + PAGE_SIZE, Ordering::SeqCst);
    
    if addr + size > VMALLOC_END {
        // 超出范围，回滚
        VMALLOC_NEXT_FREE.fetch_sub(size + PAGE_SIZE, Ordering::SeqCst);
        return 0;
    }
    
    addr
}

/// 映射 vmalloc 页面
fn map_vmalloc_page(virt: u64, phys: u64, flags: u64) -> bool {
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return false,
    };
    
    if state.page_table_mgr.is_null() {
        crate::kprintln!("map_vmalloc_page: page_table_mgr is null");
        return false;
    }
    
    unsafe { (*state.page_table_mgr).map_page(virt, phys, flags) }
}

/// 取消映射 vmalloc 页面
fn unmap_vmalloc_page(virt: u64) {
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return,
    };
    
    if state.page_table_mgr.is_null() {
        return;
    }
    
    // 使用页表管理器取消映射
    unsafe { (*state.page_table_mgr).unmap_page(virt); }
    // let _ = virt;
}

/// 获取 vmalloc 虚拟地址对应的物理地址
fn get_vmalloc_phys(virt: u64) -> Option<u64> {
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return None,
    };
    
    if state.page_table_mgr.is_null() {
        return None;
    }
    
    unsafe { (*state.page_table_mgr).translate_addr(virt) }
}

/// 页对齐 (向上)
#[inline]
const fn page_align_up(size: u64) -> u64 {
    (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

// ============================================================================
// ioremap (IO 内存映射)
// ============================================================================

/// 将物理地址映射到 vmalloc 区域 (IO 重映射)
#[track_caller]
pub fn ioremap(phys_addr: u64, size: usize) -> *mut u8 {
    if size == 0 {
        return ptr::null_mut();
    }
    
    // 对齐到页边界
    let offset = phys_addr & (PAGE_SIZE - 1);
    let phys_base = phys_addr & !(PAGE_SIZE - 1);
    let page_align_size = (size as u64 + offset + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let nr_pages = (page_align_size / PAGE_SIZE) as u32;
    
    // 分配 vmalloc 区域 (但不分配物理页)
    let vm_ptr = unsafe {
        // 找一个空闲的 VmStruct
        let mut area_ptr: *mut VmStruct = ptr::null_mut();
        for i in 0..MAX_VMALLOC_AREAS {
            if !VMALLOC_AREAS[i].flags.contains(VmFlags::ALLOC) {
                area_ptr = &mut VMALLOC_AREAS[i];
                break;
            }
        }
        
        if area_ptr.is_null() {
            crate::kprintln!("ioremap: No free VmStruct");
            return ptr::null_mut();
        }
        
        let area = &mut *area_ptr;
        
        // 分配虚拟地址空间
        let vaddr = allocate_vm_area(page_align_size);
        if vaddr == 0 {
            crate::kprintln!("ioremap: allocate_vm_area failed");
            return ptr::null_mut();
        }
        
        // 初始化 VmStruct
        area.init(vaddr, page_align_size, Some(Location::caller()));
        area.nr_pages = nr_pages;
        area.flags.set(VmFlags::IOREMAP);
        
        VMALLOC_AREA_COUNT.fetch_add(1, Ordering::Relaxed);
        
        vaddr as *mut u8
    };
    
    if vm_ptr.is_null() {
        return ptr::null_mut();
    }
    
    let vm_start = vm_ptr as u64;
    
    // 映射每一页
    for i in 0..(page_align_size / PAGE_SIZE) {
        let phys = phys_base + i * PAGE_SIZE;
        let virt = vm_start + i * PAGE_SIZE;
        
        if !map_vmalloc_page(virt, phys, PTE_PRESENT | PTE_WRITABLE | PTE_NO_CACHE) {
            crate::kprintln!("ioremap: map_vmalloc_page failed at virt={:#x} phys={:#x}", virt, phys);
            vfree(vm_ptr);
            return ptr::null_mut();
        }
    }
    
    unsafe { vm_ptr.add(offset as usize) }
}

/// 取消 IO 内存映射
pub fn iounmap(addr: *mut u8) {
    vfree(addr);
}

// ============================================================================
// 统计信息
// ============================================================================

/// vmalloc 统计信息
pub struct VmallocStats {
    /// 已分配区域数
    pub nr_areas: u64,
    /// 总虚拟内存 (字节)
    pub total_vm: u64,
    /// 总物理内存 (字节)
    pub total_phys: u64,
}

/// 获取 vmalloc 统计信息
pub fn vmalloc_stats() -> VmallocStats {
    let mut stats = VmallocStats {
        nr_areas: VMALLOC_AREA_COUNT.load(Ordering::Relaxed),
        total_vm: 0,
        total_phys: 0,
    };
    
    unsafe {
        for i in 0..MAX_VMALLOC_AREAS {
            let area = &VMALLOC_AREAS[i];
            if area.flags.contains(VmFlags::INUSE) {
                stats.total_vm += area.size;
                stats.total_phys += (area.nr_pages as u64) * PAGE_SIZE;
            }
        }
    }
    
    stats
}

/// 打印所有 vmalloc 分配信息 (用于检测泄漏)
pub fn vmalloc_dump_info() {
    crate::kprintln!("=== vmalloc dump ===");
    unsafe {
        for i in 0..MAX_VMALLOC_AREAS {
            let area = &VMALLOC_AREAS[i];
            if area.flags.contains(VmFlags::INUSE) {
                crate::kprintln!(
                    "VA: {:#x} - {:#x} ({}) | Pages: {} | Caller: {}",
                    area.addr,
                    area.addr + area.size,
                    area.size,
                    area.nr_pages,
                    if let Some(loc) = area.caller {
                        loc.file()
                    } else {
                        "unknown"
                    },
                );
                if let Some(loc) = area.caller {
                    crate::kprintln!("    at {}:{}", loc.file(), loc.line());
                }
            }
        }
    }
    crate::kprintln!("====================");
}
