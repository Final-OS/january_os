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
    PTE_GLOBAL, PTE_NO_CACHE, PTE_PRESENT, PTE_WRITABLE, PageTableManager,
};
use crate::sync::{Mutex, OnceCell};
use core::panic::Location;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

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
    /// 物理基址（仅 IOREMAP 有效；普通 vmalloc 为 0）
    pub phys_base: u64,
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
}

// 允许跨线程发送（实际上是单核初始化）
unsafe impl Send for VmallocState {}
unsafe impl Sync for VmallocState {}

/// vmalloc 全局状态（一次性初始化）
static VMALLOC_STATE: OnceCell<VmallocState> = OnceCell::new();
static WATCH_VMALLOC_PAGE_PRIMARY: AtomicU64 = AtomicU64::new(0);
static WATCH_VMALLOC_PAGE_SECONDARY: AtomicU64 = AtomicU64::new(0);
static VMALLOC_HEAL_IOREMAP_COUNT: AtomicU64 = AtomicU64::new(0);
static VMALLOC_HEAL_FROM_INIT_COUNT: AtomicU64 = AtomicU64::new(0);
static VMALLOC_HEAL_MISS_COUNT: AtomicU64 = AtomicU64::new(0);
static VMALLOC_HEAL_FAIL_COUNT: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn set_vmalloc_watch_page(addr: u64) {
    if !crate::config::DEBUG_VERBOSE {
        return;
    }
    let page = addr & !(PAGE_SIZE - 1);
    let primary = WATCH_VMALLOC_PAGE_PRIMARY.load(Ordering::Acquire);
    if primary == page {
        return;
    }
    if primary == 0 {
        WATCH_VMALLOC_PAGE_PRIMARY.store(page, Ordering::Release);
        return;
    }
    let secondary = WATCH_VMALLOC_PAGE_SECONDARY.load(Ordering::Acquire);
    if secondary == page {
        return;
    }
    if secondary == 0 {
        WATCH_VMALLOC_PAGE_SECONDARY.store(page, Ordering::Release);
        return;
    }
    // 保留最近观察到的两个热点页。
    WATCH_VMALLOC_PAGE_SECONDARY.store(page, Ordering::Release);
}

#[inline]
pub fn is_vmalloc_watch_page(addr: u64) -> bool {
    let page = addr & !(PAGE_SIZE - 1);
    let primary = WATCH_VMALLOC_PAGE_PRIMARY.load(Ordering::Acquire);
    if primary != 0 && page == primary {
        return true;
    }
    let secondary = WATCH_VMALLOC_PAGE_SECONDARY.load(Ordering::Acquire);
    secondary != 0 && page == secondary
}

pub struct VmallocHealStats {
    pub recovered_from_ioremap: u64,
    pub recovered_from_init: u64,
    pub heal_miss: u64,
    pub heal_fail: u64,
}

#[inline]
pub fn vmalloc_heal_stats() -> VmallocHealStats {
    VmallocHealStats {
        recovered_from_ioremap: VMALLOC_HEAL_IOREMAP_COUNT.load(Ordering::Relaxed),
        recovered_from_init: VMALLOC_HEAL_FROM_INIT_COUNT.load(Ordering::Relaxed),
        heal_miss: VMALLOC_HEAL_MISS_COUNT.load(Ordering::Relaxed),
        heal_fail: VMALLOC_HEAL_FAIL_COUNT.load(Ordering::Relaxed),
    }
}

/// 只读查询：返回 vmalloc 地址在 current/init root 的映射状态。
pub fn vmalloc_mapping_state(addr: u64) -> Option<(u64, u64, Option<u64>, Option<u64>)> {
    if !crate::mm::is_vmalloc_addr(addr) {
        return None;
    }
    let state = VMALLOC_STATE.get()?;
    let page_virt = addr & !(PAGE_SIZE - 1);

    unsafe fn translate_in_root(root_phys: u64, direct_map: u64, virt: u64) -> Option<u64> {
        if root_phys == 0 {
            return None;
        }
        let pt_mgr = PageTableManager::new_with_layout(
            root_phys,
            direct_map,
            crate::mm::page_levels(),
            crate::mm::va_bits(),
        );
        pt_mgr.translate_addr(virt)
    }

    unsafe {
        let current_root = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        let init_root = (*crate::mm::init_mm_ptr()).pgd & crate::mm::PTE_ADDR_MASK;
        let current_phys = translate_in_root(current_root, state.direct_map, page_virt);
        let init_phys = translate_in_root(init_root, state.direct_map, page_virt);
        Some((current_root, init_root, current_phys, init_phys))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RootMapOutcome {
    Unchanged,
    Mapped,
}

unsafe fn map_page_in_root_checked(
    root_phys: u64,
    direct_map: u64,
    virt: u64,
    phys: u64,
    flags: u64,
) -> Option<RootMapOutcome> {
    if root_phys == 0 {
        return None;
    }
    let pt_mgr = PageTableManager::new_with_layout(
        root_phys,
        direct_map,
        crate::mm::page_levels(),
        crate::mm::va_bits(),
    );
    let phys_page = phys & !(PAGE_SIZE - 1);
    if let Some(existing_phys) = pt_mgr.translate_addr(virt).map(|p| p & !(PAGE_SIZE - 1)) {
        return if existing_phys == phys_page {
            Some(RootMapOutcome::Unchanged)
        } else {
            None
        };
    }

    if !pt_mgr.map_page(virt, phys_page, flags) {
        return None;
    }
    pt_mgr.flush_tlb(virt);

    if pt_mgr.translate_addr(virt).map(|p| p & !(PAGE_SIZE - 1)) == Some(phys_page) {
        Some(RootMapOutcome::Mapped)
    } else {
        let _ = pt_mgr.unmap_page(virt);
        None
    }
}

unsafe fn unmap_page_in_root(root_phys: u64, direct_map: u64, virt: u64) {
    if root_phys == 0 {
        return;
    }
    let pt_mgr = PageTableManager::new_with_layout(
        root_phys,
        direct_map,
        crate::mm::page_levels(),
        crate::mm::va_bits(),
    );
    let _ = pt_mgr.unmap_page(virt);
}

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 vmalloc 子系统
pub unsafe fn init_vmalloc(direct_map: u64) {
    let _ = VMALLOC_STATE.set(VmallocState { direct_map });
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
            phys_base: 0,
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

    unsafe {
        let current_root = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        let init_root = (*crate::mm::init_mm_ptr()).pgd & crate::mm::PTE_ADDR_MASK;

        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[vmalloc] map roots current={:#x} init={:#x}",
                current_root,
                init_root
            );
        }

        let init_outcome = if init_root != 0 {
            match map_page_in_root_checked(init_root, state.direct_map, virt, phys, flags) {
                Some(outcome) => outcome,
                None => {
                    crate::kprintln!(
                        "map_vmalloc_page: map failed in init root virt={:#x} phys={:#x} root={:#x}",
                        virt,
                        phys,
                        init_root
                    );
                    return false;
                }
            }
        } else {
            RootMapOutcome::Unchanged
        };

        if current_root != 0 && current_root != init_root {
            if map_page_in_root_checked(current_root, state.direct_map, virt, phys, flags).is_none()
            {
                if init_outcome == RootMapOutcome::Mapped {
                    unmap_page_in_root(init_root, state.direct_map, virt);
                }
                crate::kprintln!(
                    "map_vmalloc_page: map failed in current root virt={:#x} phys={:#x} root={:#x}",
                    virt,
                    phys,
                    current_root
                );
                return false;
            }
        }

        true
    }
}

/// 取消映射 vmalloc 页面
fn unmap_vmalloc_page(virt: u64) {
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return,
    };

    unsafe {
        let current_root = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        let init_root = (*crate::mm::init_mm_ptr()).pgd & crate::mm::PTE_ADDR_MASK;
        unmap_page_in_root(init_root, state.direct_map, virt);
        if current_root != init_root {
            unmap_page_in_root(current_root, state.direct_map, virt);
        }
    }
}

/// 获取 vmalloc 虚拟地址对应的物理地址
fn get_vmalloc_phys(virt: u64) -> Option<u64> {
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return None,
    };

    unsafe fn translate_in_root(root_phys: u64, direct_map: u64, virt: u64) -> Option<u64> {
        if root_phys == 0 {
            return None;
        }
        let pt_mgr = PageTableManager::new_with_layout(
            root_phys,
            direct_map,
            crate::mm::page_levels(),
            crate::mm::va_bits(),
        );
        pt_mgr.translate_addr(virt)
    }

    unsafe {
        let current_root = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        if let Some(phys) = translate_in_root(current_root, state.direct_map, virt) {
            return Some(phys);
        }
        let init_root = (*crate::mm::init_mm_ptr()).pgd & crate::mm::PTE_ADDR_MASK;
        translate_in_root(init_root, state.direct_map, virt)
    }
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
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[ioremap] enter phys={:#x} size={}",
            phys_addr,
            size
        );
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
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[ioremap] find_gap alloc_size={:#x}",
                alloc_size
            );
        }
        data.ensure_addr_space();
        let vaddr = match data.addr_space.as_mut().unwrap().find_gap(
            alloc_size_usize,
            vmalloc_start() as usize,
            vmalloc_end() as usize,
        ) {
            Some(addr) => {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[ioremap] find_gap ok vaddr={:#x}",
                        addr as u64
                    );
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
            phys_base,
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
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[ioremap] mapping pages count={}",
            nr_pages_u64
        );
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

/// 确保 vmalloc 页在当前 CR3 下可见。
///
/// 某些路径会在非 init_mm 的私有地址空间里访问内核 vmalloc/ioremap 地址；
/// 若当前根页表缺少该页映射，则从 init_mm 对应条目补齐到当前根页表。
pub fn ensure_vmalloc_page_mapped_in_current(addr: u64) -> bool {
    if !crate::mm::is_vmalloc_addr(addr) {
        return true;
    }
    let state = match VMALLOC_STATE.get() {
        Some(s) => s,
        None => return false,
    };

    let page_virt = addr & !(PAGE_SIZE - 1);

    #[inline]
    fn lookup_ioremap_phys(page_virt: u64) -> Option<(u64, u64)> {
        let data = VMALLOC_DATA.lock();
        for (&start, area) in data.areas.iter() {
            let end = start.saturating_add(area.size);
            if page_virt >= start
                && page_virt < end
                && area.flags.contains(VmFlags::IOREMAP)
                && area.phys_base != 0
            {
                let page_offset = page_virt.saturating_sub(start);
                let phys = area.phys_base.saturating_add(page_offset);
                return Some((start, phys));
            }
        }
        None
    }

    #[inline]
    fn heal_from_ioremap_metadata(
        page_virt: u64,
        direct_map: u64,
        current_root: u64,
        init_root: u64,
    ) -> bool {
        let Some((_area_start, ioremap_phys)) = lookup_ioremap_phys(page_virt) else {
            VMALLOC_HEAL_MISS_COUNT.fetch_add(1, Ordering::Relaxed);
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[vmalloc] heal miss: no ioremap metadata virt={:#x} current_root={:#x} init_root={:#x}",
                    page_virt,
                    current_root,
                    init_root
                );
            }
            return false;
        };

        let flags = PTE_PRESENT | PTE_WRITABLE | PTE_NO_CACHE;
        let init_outcome = match unsafe {
            map_page_in_root_checked(init_root, direct_map, page_virt, ioremap_phys, flags)
        } {
            Some(outcome) => outcome,
            None => {
                VMALLOC_HEAL_FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        };

        if current_root != init_root
            && unsafe {
                map_page_in_root_checked(current_root, direct_map, page_virt, ioremap_phys, flags)
            }
            .is_none()
        {
            if init_outcome == RootMapOutcome::Mapped {
                unsafe { unmap_page_in_root(init_root, direct_map, page_virt) };
            }
            VMALLOC_HEAL_FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
            return false;
        }

        VMALLOC_HEAL_IOREMAP_COUNT.fetch_add(1, Ordering::Relaxed);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[vmalloc] healed from ioremap metadata virt={:#x} phys={:#x} current_root={:#x} init_root={:#x}",
                page_virt,
                ioremap_phys,
                current_root,
                init_root
            );
        }
        true
    }

    unsafe {
        let current_root = crate::mm::arch::read_cr3() & crate::mm::PTE_ADDR_MASK;
        let init_root = (*crate::mm::init_mm_ptr()).pgd & crate::mm::PTE_ADDR_MASK;
        if current_root == 0 || init_root == 0 {
            return false;
        }

        let current_pt = PageTableManager::new_with_layout(
            current_root,
            state.direct_map,
            crate::mm::page_levels(),
            crate::mm::va_bits(),
        );
        if current_pt.translate_addr(page_virt).is_some() {
            return true;
        }

        let init_pt = PageTableManager::new_with_layout(
            init_root,
            state.direct_map,
            crate::mm::page_levels(),
            crate::mm::va_bits(),
        );

        let init_entry = init_pt.translate(page_virt).map(|(entry, _, _)| entry);
        let init_phys_with_off = init_pt.translate_addr(page_virt);

        let (Some(entry), Some(phys_with_off)) = (init_entry, init_phys_with_off) else {
            // init_root 缺失该页映射，尝试从 ioremap 元数据重建。
            return heal_from_ioremap_metadata(
                page_virt,
                state.direct_map,
                current_root,
                init_root,
            );
        };
        let phys_page = phys_with_off & !(PAGE_SIZE - 1);
        let flags = entry.flags();

        let ok = match map_page_in_root_checked(
            current_root,
            state.direct_map,
            page_virt,
            phys_page,
            flags,
        ) {
            Some(outcome) => {
                if outcome == RootMapOutcome::Mapped {
                    VMALLOC_HEAL_FROM_INIT_COUNT.fetch_add(1, Ordering::Relaxed);
                }
                true
            }
            None => {
                VMALLOC_HEAL_FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
                false
            }
        };
        if ok && crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[vmalloc] healed missing mapping virt={:#x} phys={:#x} current_root={:#x} init_root={:#x}",
                page_virt,
                phys_page,
                current_root,
                init_root
            );
        }
        ok
    }
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
