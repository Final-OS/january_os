// ============================================================================
// january_os - vmalloc 虚拟连续内存分配
//
// 分配虚拟地址连续但物理地址不连续的内存
// 用于大块内核内存分配（如模块、大缓冲区等）
// ============================================================================

use crate::libs::mptree::MapleTree;
use crate::libs::rbtree::RbTree;
use crate::mm::page::buddy::{alloc_page, free_page};
use crate::mm::page::page::{page_to_pfn, pfn_to_page};
use crate::mm::page::zone::GfpFlags;
use crate::mm::vm::layout::PAGE_SIZE;
use crate::mm::vm::paging::{
    PageTableManager, PTE_GLOBAL, PTE_NO_CACHE, PTE_PRESENT, PTE_WRITABLE,
};
use crate::sync::{Mutex, OnceCell};
use core::panic::Location;
use core::ptr;

// ============================================================================
// 常量
// ============================================================================

/// vmalloc 区域起始地址 (内核高地址空间)
pub const VMALLOC_START: u64 = crate::config::VMALLOC_START;

/// vmalloc 区域结束地址
pub const VMALLOC_END: u64 = crate::config::VMALLOC_END;

/// vmalloc 区域大小
pub const VMALLOC_SIZE: u64 = VMALLOC_END - VMALLOC_START;

#[inline]
fn vmalloc_start() -> u64 {
    crate::mm::vmalloc_start()
}

#[inline]
fn vmalloc_end() -> u64 {
    crate::mm::vmalloc_end()
}

// ============================================================================
// vmalloc 区域描述
// ============================================================================

/// vmalloc 区域标志
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct VmFlags(u32);

impl VmFlags {
    /// 已分配
    pub const ALLOC: u32 = 1 << 0;
    /// 正在使用
    pub const INUSE: u32 = 1 << 1;
    /// 用户映射
    pub const USERMAP: u32 = 1 << 2;
    /// IO 映射
    pub const IOREMAP: u32 = 1 << 3;
    /// 分配进行中（页映射尚未完成）
    pub const ALLOCATING: u32 = 1 << 4;

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

/// vmalloc 区域信息 (存储在 RbTree 中，以 vaddr 为键)
pub struct VmArea {
    /// 大小 (字节)
    pub size: u64,
    /// 标志
    pub flags: VmFlags,
    /// 页数
    pub nr_pages: u32,
    /// 调用者信息 (调试用)
    pub caller: Option<&'static Location<'static>>,
}

// ============================================================================
// 全局状态
// ============================================================================

/// vmalloc 数据 (受 Mutex 保护)
struct VmallocData {
    /// 区域查找 (vaddr -> VmArea)
    areas: RbTree<u64, VmArea>,
    /// 地址空间跟踪 (用于间隙搜索)
    addr_space: Option<MapleTree<()>>,
}

impl VmallocData {
    const fn new() -> Self {
        Self {
            areas: RbTree::new(),
            addr_space: None,
        }
    }

    fn ensure_addr_space(&mut self) {
        if self.addr_space.is_none() {
            self.addr_space = Some(MapleTree::new());
        }
    }
}

/// vmalloc 受保护数据
static VMALLOC_DATA: Mutex<VmallocData> = Mutex::new(VmallocData::new());

/// vmalloc 状态
struct VmallocState {
    /// 直接映射偏移
    direct_map: u64,
    /// 页表管理器指针
    page_table_mgr: *const PageTableManager,
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
pub unsafe fn init_vmalloc(direct_map: u64, pt_mgr: *const PageTableManager) {
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

    // 从 RbTree 中查找并移除
    let area = {
        let mut data = VMALLOC_DATA.lock();
        // 检查是否正在分配中，如果是则不能释放
        if let Some(area) = data.areas.get(&addr) {
            if area.flags.contains(VmFlags::ALLOCATING) {
                return;
            }
        }
        let area = data.areas.remove(&addr);
        if area.is_some() {
            // 同时从地址空间跟踪中移除
            data.ensure_addr_space();
            data.addr_space.as_mut().unwrap().remove(addr as usize);
        }
        area
    };

    if let Some(area) = area {
        unsafe {
            __vfree(addr, &area);
        }
    }
}

/// 内部分配函数
#[track_caller]
fn __vmalloc(size: usize, gfp: GfpFlags) -> *mut u8 {
    // 计算需要的页数（带溢出保护）
    let size = match (size as u64)
        .checked_add(PAGE_SIZE - 1)
        .map(|v| v & !(PAGE_SIZE - 1))
    {
        Some(v) if v != 0 => v,
        _ => return ptr::null_mut(),
    };

    let nr_pages_u64 = size / PAGE_SIZE;
    if nr_pages_u64 == 0 || nr_pages_u64 > u32::MAX as u64 {
        return ptr::null_mut();
    }
    let nr_pages = nr_pages_u64 as u32;

    // 分配虚拟地址空间并注册区域 (持有锁)
    let vaddr = {
        let mut data = VMALLOC_DATA.lock();

        // 使用 MapleTree 间隙搜索分配虚拟地址
        // 每个区域之间留一个 guard page
        let alloc_size = match size.checked_add(PAGE_SIZE) {
            Some(v) => v,
            None => return ptr::null_mut(),
        };
        let alloc_size_usize = match usize::try_from(alloc_size) {
            Ok(v) => v,
            Err(_) => return ptr::null_mut(),
        };
        data.ensure_addr_space();
        let vaddr = match data.addr_space.as_mut().unwrap().find_gap(
            alloc_size_usize,
            vmalloc_start() as usize,
            vmalloc_end() as usize,
        ) {
            Some(addr) => addr as u64,
            None => return ptr::null_mut(),
        };

        let end = match vaddr.checked_add(alloc_size) {
            Some(v) => v,
            None => return ptr::null_mut(),
        };
        let end_usize = match usize::try_from(end) {
            Ok(v) => v,
            Err(_) => return ptr::null_mut(),
        };

        // 注册到地址空间跟踪
        data.ensure_addr_space();
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[ioremap] addr_space insert [{:#x}, {:#x})",
                vaddr,
                end
            );
        }
        if data
            .addr_space
            .as_mut()
            .unwrap()
            .insert(vaddr as usize, end_usize, ())
            .is_err()
        {
            return ptr::null_mut();
        }

        // 注册到区域 RbTree
        let area = VmArea {
            size,
            flags: VmFlags::new(VmFlags::ALLOC | VmFlags::INUSE | VmFlags::ALLOCATING),
            nr_pages,
            caller: Some(Location::caller()),
        };
        data.areas.insert(vaddr, area);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[ioremap] area inserted vaddr={:#x} pages={}",
                vaddr,
                nr_pages
            );
        }

        vaddr
    };
    // 锁已释放

    // 分配物理页并映射 (不持有 VMALLOC_DATA 锁)
    unsafe {
        for i in 0..nr_pages {
            let page = match alloc_page(gfp) {
                Some(p) => p,
                None => {
                    // 分配失败，释放已映射的页
                    cleanup_partial(vaddr, i, nr_pages);
                    return ptr::null_mut();
                }
            };

            let phys = page_to_pfn(page) * PAGE_SIZE;
            let virt = vaddr + (i as u64) * PAGE_SIZE;

            // 映射页面
            if !map_vmalloc_page(virt, phys, PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL) {
                // 映射失败
                free_page(page);
                cleanup_partial(vaddr, i, nr_pages);
                return ptr::null_mut();
            }
        }
    }

    // 映射完成，清除 ALLOCATING 标志
    {
        let mut data = VMALLOC_DATA.lock();
        if let Some(area) = data.areas.get_mut(&vaddr) {
            area.flags.clear(VmFlags::ALLOCATING);
        }
    }

    vaddr as *mut u8
}

/// 清理部分分配 (分配失败时)
unsafe fn cleanup_partial(vaddr: u64, mapped_pages: u32, _total_pages: u32) {
    // 取消已映射的页并释放
    for j in 0..mapped_pages {
        let virt = vaddr + (j as u64) * PAGE_SIZE;
        if let Some(phys) = get_vmalloc_phys(virt) {
            unmap_vmalloc_page(virt);
            let pfn = phys / PAGE_SIZE;
            free_page(&mut *pfn_to_page(pfn));
        }
    }

    // 从跟踪结构中移除
    let mut data = VMALLOC_DATA.lock();
    data.areas.remove(&vaddr);
    data.ensure_addr_space();
    data.addr_space.as_mut().unwrap().remove(vaddr as usize);
}

/// 内部释放函数 (在锁外调用，area 已从树中移除)
unsafe fn __vfree(addr: u64, area: &VmArea) {
    // 取消映射并释放物理页
    for i in 0..area.nr_pages {
        let virt = addr + (i as u64) * PAGE_SIZE;

        // 获取物理地址
        if let Some(phys) = get_vmalloc_phys(virt) {
            // 取消映射
            unmap_vmalloc_page(virt);

            // 释放物理页 (仅当不是 IO 映射时)
            if !area.flags.contains(VmFlags::IOREMAP) {
                let pfn = phys / PAGE_SIZE;
                free_page(&mut *pfn_to_page(pfn));
            }
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 映射 vmalloc 页面
fn map_vmalloc_page(virt: u64, phys: u64, flags: u64) -> bool {
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[vmalloc] map_page enter virt={:#x} phys={:#x}",
            virt,
            phys
        );
    }
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return false,
    };

    if state.page_table_mgr.is_null() {
        crate::kprintln!("map_vmalloc_page: page_table_mgr is null");
        return false;
    }

    unsafe {
        let pt_mgr = &*state.page_table_mgr;

        // 验证 PML4 与当前 CR3 一致
        let cr3_phys = crate::mm::arch::read_cr3() & 0x000F_FFFF_FFFF_F000;
        if pt_mgr.pml4_phys() != cr3_phys {
            crate::kprintln!(
                "map_vmalloc_page: PML4 mismatch! stored={:#x} cr3={:#x}",
                pt_mgr.pml4_phys(),
                cr3_phys
            );
            return false;
        }

        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!("\x1b[90m[diag]\x1b[0m[vmalloc] calling pt_mgr.map_page");
        }
        let ok = pt_mgr.map_page(virt, phys, flags);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!("\x1b[90m[diag]\x1b[0m[vmalloc] pt_mgr.map_page -> {}", ok);
        }
        if ok {
            // map_page 对“新增映射”仅做本地刷新；vmalloc/ioremap 为低频路径，
            // 这里追加一次跨核 TLB shootdown 保障内核映射可见性一致。
            pt_mgr.flush_tlb(virt);
            // 验证映射是否生效
            if pt_mgr.translate_addr(virt).is_none() {
                crate::kprintln!(
                    "map_vmalloc_page: mapping verification FAILED virt={:#x} phys={:#x}",
                    virt,
                    phys
                );
                return false;
            }
        }
        ok
    }
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
    unsafe {
        (&*state.page_table_mgr).unmap_page(virt);
    }
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

    unsafe { (&*state.page_table_mgr).translate_addr(virt) }
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
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("\x1b[90m[diag]\x1b[0m[ioremap] enter phys={:#x} size={}", phys_addr, size);
    }
    if size == 0 {
        return ptr::null_mut();
    }

    // 对齐到页边界
    let offset = phys_addr & (PAGE_SIZE - 1);
    let phys_base = phys_addr & !(PAGE_SIZE - 1);
    let page_align_size = match (size as u64)
        .checked_add(offset)
        .and_then(|v| v.checked_add(PAGE_SIZE - 1))
        .map(|v| v & !(PAGE_SIZE - 1))
    {
        Some(v) if v != 0 => v,
        _ => {
            crate::kprintln!("ioremap: size overflow");
            return ptr::null_mut();
        }
    };
    let nr_pages_u64 = page_align_size / PAGE_SIZE;
    if nr_pages_u64 == 0 || nr_pages_u64 > u32::MAX as u64 {
        crate::kprintln!("ioremap: invalid page count {}", nr_pages_u64);
        return ptr::null_mut();
    }
    let nr_pages = nr_pages_u64 as u32;

    // 分配虚拟地址空间并注册区域 (持有锁)
    let vaddr = {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!("\x1b[90m[diag]\x1b[0m[ioremap] lock vmalloc_data");
        }
        let mut data = VMALLOC_DATA.lock();

        let alloc_size = match page_align_size.checked_add(PAGE_SIZE) {
            Some(v) => v,
            None => {
                crate::kprintln!("ioremap: alloc_size overflow");
                return ptr::null_mut();
            }
        }; // guard page
        let alloc_size_usize = match usize::try_from(alloc_size) {
            Ok(v) => v,
            Err(_) => {
                crate::kprintln!("ioremap: alloc_size too large");
                return ptr::null_mut();
            }
        };
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!("\x1b[90m[diag]\x1b[0m[ioremap] find_gap alloc_size={:#x}", alloc_size);
        }
        data.ensure_addr_space();
        let vaddr = match data.addr_space.as_mut().unwrap().find_gap(
            alloc_size_usize,
            vmalloc_start() as usize,
            vmalloc_end() as usize,
        ) {
            Some(addr) => {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!("\x1b[90m[diag]\x1b[0m[ioremap] find_gap ok vaddr={:#x}", addr as u64);
                }
                addr as u64
            }
            None => {
                crate::kprintln!("ioremap: find_gap failed");
                return ptr::null_mut();
            }
        };

        let end = match vaddr.checked_add(alloc_size) {
            Some(v) => v,
            None => {
                crate::kprintln!("ioremap: vaddr end overflow");
                return ptr::null_mut();
            }
        };
        let end_usize = match usize::try_from(end) {
            Ok(v) => v,
            Err(_) => {
                crate::kprintln!("ioremap: vaddr end out of range");
                return ptr::null_mut();
            }
        };

        data.ensure_addr_space();
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[ioremap] addr_space insert [{:#x}, {:#x})",
                vaddr,
                end
            );
        }
        if data
            .addr_space
            .as_mut()
            .unwrap()
            .insert(vaddr as usize, end_usize, ())
            .is_err()
        {
            crate::kprintln!("ioremap: addr_space insert failed");
            return ptr::null_mut();
        }

        let mut flags = VmFlags::new(VmFlags::ALLOC | VmFlags::INUSE);
        flags.set(VmFlags::IOREMAP);

        let area = VmArea {
            size: page_align_size,
            flags,
            nr_pages,
            caller: Some(Location::caller()),
        };
        data.areas.insert(vaddr, area);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[ioremap] area inserted vaddr={:#x} pages={}",
                vaddr,
                nr_pages
            );
        }

        vaddr
    };
    // 锁已释放

    // 映射每一页 (不持有 VMALLOC_DATA 锁)
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("\x1b[90m[diag]\x1b[0m[ioremap] mapping pages count={}", nr_pages_u64);
    }
    for i in 0..nr_pages_u64 {
        let phys = phys_base + i * PAGE_SIZE;
        let virt = vaddr + i * PAGE_SIZE;

        if i == 0 {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[ioremap] map first virt={:#x} phys={:#x}",
                    virt,
                    phys
                );
            }
        }

        if !map_vmalloc_page(virt, phys, PTE_PRESENT | PTE_WRITABLE | PTE_NO_CACHE) {
            crate::kprintln!(
                "ioremap: map_vmalloc_page failed at virt={:#x} phys={:#x}",
                virt,
                phys
            );
            vfree(vaddr as *mut u8);
            return ptr::null_mut();
        }
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[ioremap] success vaddr={:#x} offset={:#x}",
            vaddr,
            offset
        );
    }
    unsafe { (vaddr as *mut u8).add(offset as usize) }
}

/// 取消 IO 内存映射
pub fn iounmap(addr: *mut u8) {
    if addr.is_null() {
        return;
    }
    // ioremap 返回 vaddr + offset，需要页对齐后再查找
    let aligned = ((addr as u64) & !(PAGE_SIZE - 1)) as *mut u8;
    vfree(aligned);
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
    let data = VMALLOC_DATA.lock();
    let mut stats = VmallocStats {
        nr_areas: data.areas.len() as u64,
        total_vm: 0,
        total_phys: 0,
    };

    for (_, area) in data.areas.iter() {
        stats.total_vm += area.size;
        stats.total_phys += (area.nr_pages as u64) * PAGE_SIZE;
    }

    stats
}

/// 打印所有 vmalloc 分配信息 (用于检测泄漏)
pub fn vmalloc_dump_info() {
    crate::kprintln!("=== vmalloc dump ===");
    let data = VMALLOC_DATA.lock();
    for (&addr, area) in data.areas.iter() {
        crate::kprintln!(
            "VA: {:#x} - {:#x} ({}) | Pages: {} | Caller: {}",
            addr,
            addr + area.size,
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
    crate::kprintln!("====================");
}
