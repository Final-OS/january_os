// ============================================================================
// january_os - 页错误处理
//
// 处理页错误异常，实现 demand paging 和 COW
// ============================================================================

use super::layout::PAGE_SIZE;
use super::paging::PageTableManager;
use super::vma::{Mm, VmFlags};
use crate::fs;
use crate::mm::page::buddy::{alloc_page, free_page};
use crate::mm::page::page::{PageFlags, page_to_pfn, pfn_to_page};
use crate::mm::page::zone::GfpFlags;
use core::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// 页错误类型
// ============================================================================

/// 页错误错误码 (x86_64 CR2)
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageFaultError(pub u64);

impl PageFaultError {
    /// 保护违规 (vs 页不存在)
    pub const PRESENT: u64 = 1 << 0;
    /// 写访问 (vs 读访问)
    pub const WRITE: u64 = 1 << 1;
    /// 用户态 (vs 内核态)
    pub const USER: u64 = 1 << 2;
    /// 保留位被设置
    pub const RSVD: u64 = 1 << 3;
    /// 取指访问
    pub const INSTR: u64 = 1 << 4;
    /// 保护密钥违规
    pub const PK: u64 = 1 << 5;
    /// 影子栈访问
    pub const SS: u64 = 1 << 6;

    pub fn new(code: u64) -> Self {
        Self(code)
    }

    /// 是否是保护违规 (页存在但权限不足)
    pub fn is_protection_violation(&self) -> bool {
        (self.0 & Self::PRESENT) != 0
    }

    /// 是否是写访问
    pub fn is_write(&self) -> bool {
        (self.0 & Self::WRITE) != 0
    }

    /// 是否是用户态访问
    pub fn is_user(&self) -> bool {
        (self.0 & Self::USER) != 0
    }

    /// 是否是取指访问
    pub fn is_instruction_fetch(&self) -> bool {
        (self.0 & Self::INSTR) != 0
    }
}

// ============================================================================
// 页错误结果
// ============================================================================

/// 页错误处理结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultResult {
    /// 成功处理，可以重试
    Retry,
    /// 信号 SIGSEGV
    Sigsegv,
    /// 信号 SIGBUS
    Sigbus,
    /// 内核 oops
    KernelOops,
    /// OOM (内存不足)
    Oom,
}

/// 页错误详细类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultType {
    /// 页不存在，需要分配
    NotPresent,
    /// 写时复制
    CopyOnWrite,
    /// 保护违规
    Protection,
    /// 栈扩展
    StackGrowth,
    /// 文件映射缺页
    FileFault,
    /// 匿名映射缺页
    AnonFault,
}

// ============================================================================
// 页错误处理上下文
// ============================================================================

/// 页错误上下文
pub struct FaultContext {
    /// 错误地址
    pub address: u64,
    /// 错误码
    pub error_code: PageFaultError,
    /// 当前 mm
    pub mm: *mut Mm,
    /// VMA 起始地址
    pub vma_start: u64,
    /// VMA 结束地址
    pub vma_end: u64,
    /// VMA 标志
    pub vma_flags: VmFlags,
    /// 文件偏移（页单位）
    pub vma_pgoff: u64,
    /// 文件映射后备句柄（由 fs 层解释）
    pub vma_file: *mut (),
    /// 文件映射私有字段（由 fs 层解释）
    pub vma_private_data: *mut (),
    /// 直接映射偏移
    pub direct_map_offset: u64,
}

impl FaultContext {
    pub fn new(address: u64, error_code: u64, mm: *mut Mm, direct_map: u64) -> Self {
        Self {
            address,
            error_code: PageFaultError::new(error_code),
            mm,
            vma_start: 0,
            vma_end: 0,
            vma_flags: VmFlags::empty(),
            vma_pgoff: 0,
            vma_file: core::ptr::null_mut(),
            vma_private_data: core::ptr::null_mut(),
            direct_map_offset: direct_map,
        }
    }
}

// ============================================================================
// 主处理函数
// ============================================================================

/// 处理页错误
///
/// # Arguments
/// * `ctx` - 页错误上下文
///
/// # Returns
/// 处理结果
pub fn handle_page_fault(ctx: &mut FaultContext) -> FaultResult {
    FAULT_STATS.total_faults.fetch_add(1, Ordering::Relaxed);

    let address = ctx.address;
    let error = ctx.error_code;

    // 1. 内核地址空间错误
    if address >= 0xFFFF_8000_0000_0000 {
        return handle_kernel_fault(ctx);
    }

    // 2. 用户地址空间错误
    if ctx.mm.is_null() {
        // 没有 mm，不应该发生在用户空间
        return FaultResult::KernelOops;
    }

    unsafe {
        let mm = &mut *ctx.mm;

        // 3. 查找 VMA
        let vma = match mm.find_vma(address) {
            Some(v) => v,
            None => {
                // 尝试按地址空间策略扩展用户栈
                if let Some(expanded) = mm.expand_stack_for_fault(address) {
                    FAULT_STATS.stack_grows.fetch_add(1, Ordering::Relaxed);
                    expanded
                } else {
                    return FaultResult::Sigsegv;
                }
            }
        };

        // 填充上下文中的 VMA 信息
        ctx.vma_start = vma.vm_start;
        ctx.vma_end = vma.vm_end;
        ctx.vma_flags = vma.vm_flags;
        ctx.vma_pgoff = vma.vm_pgoff;
        ctx.vma_file = vma.vm_file;
        ctx.vma_private_data = vma.vm_private_data;

        // 4. 检查权限
        if !check_access_permissions(&vma.vm_flags, &error) {
            return FaultResult::Sigsegv;
        }

        // 5. 处理具体错误类型
        if !error.is_protection_violation() {
            // 页不存在
            return handle_not_present(ctx);
        } else if error.is_write() {
            // 写保护，可能是 COW
            return handle_write_protection(ctx);
        } else {
            // 其他保护违规
            return FaultResult::Sigsegv;
        }
    }
}

/// 处理内核空间页错误
fn handle_kernel_fault(ctx: &FaultContext) -> FaultResult {
    let address = ctx.address;

    // vmalloc 区域
    if crate::mm::is_vmalloc_addr(address) {
        // vmalloc 区域的页错误通常是 bug
        return FaultResult::KernelOops;
    }

    // 直接映射区域
    let direct_map_start = ctx.direct_map_offset;
    let direct_map_end = crate::mm::direct_map_end();
    if address >= direct_map_start && address < direct_map_end {
        // 直接映射应该始终存在
        return FaultResult::KernelOops;
    }

    // 其他内核地址
    FaultResult::KernelOops
}

/// 检查访问权限
fn check_access_permissions(flags: &VmFlags, error: &PageFaultError) -> bool {
    // 写访问检查
    if error.is_write() && !flags.is_write() {
        return false;
    }

    // 执行访问检查
    if error.is_instruction_fetch() && !flags.is_exec() {
        return false;
    }

    // 读访问检查
    if !error.is_write() && !error.is_instruction_fetch() && !flags.is_read() {
        return false;
    }

    true
}

// ============================================================================
// 具体错误类型处理
// ============================================================================

/// 处理页不存在错误 (demand paging)
fn handle_not_present(ctx: &FaultContext) -> FaultResult {
    if ctx.vma_flags.is_anonymous() {
        // 匿名映射：分配零页
        handle_anonymous_fault(ctx)
    } else {
        // 文件映射：从文件读取
        handle_file_fault(ctx)
    }
}

/// 处理匿名页错误
fn handle_anonymous_fault(ctx: &FaultContext) -> FaultResult {
    map_zero_page(ctx, true)
}

fn map_zero_page(ctx: &FaultContext, mark_anon: bool) -> FaultResult {
    let address = ctx.address & !(PAGE_SIZE - 1); // 页对齐

    // 分配新页
    let page = match alloc_page(GfpFlags::new(GfpFlags::USER | GfpFlags::ZERO)) {
        Some(p) => p,
        None => return FaultResult::Oom,
    };

    // 设置页标志
    page.set_flag(PageFlags::UPTODATE);
    if mark_anon {
        page.set_flag(PageFlags::ANON);
    }

    // 映射页面
    let phys = page_to_pfn(page) * PAGE_SIZE;
    let pte_flags = ctx.vma_flags.to_user_pte_flags();

    // 实际映射到页表
    unsafe {
        let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);
        if !pt_mgr.map_page(address, phys, pte_flags) {
            free_page(page);
            return FaultResult::Oom;
        }
    }

    // 页已映射到页表，增加 mapcount
    page.inc_mapcount();

    // 零页按需分配不涉及外部 I/O，按 minor fault 统计。
    FAULT_STATS.minor_faults.fetch_add(1, Ordering::Relaxed);

    FaultResult::Retry
}

/// 处理文件映射页错误
fn handle_file_fault(ctx: &FaultContext) -> FaultResult {
    let address = ctx.address & !(PAGE_SIZE - 1);

    if !ctx.vma_file.is_null() {
        let backing_id = ctx.vma_file as usize as u64;
        if backing_id == 0 {
            return FaultResult::Sigbus;
        }

        let page_in_vma = match address.checked_sub(ctx.vma_start) {
            Some(delta) => delta / PAGE_SIZE,
            None => return FaultResult::Sigbus,
        };
        let file_offset = match ctx
            .vma_pgoff
            .checked_add(page_in_vma)
            .and_then(|p| p.checked_mul(PAGE_SIZE))
            .map(|off| off as usize)
        {
            Some(off) => off,
            None => return FaultResult::Sigbus,
        };

        let page = match alloc_page(GfpFlags::new(GfpFlags::USER | GfpFlags::ZERO)) {
            Some(p) => p,
            None => return FaultResult::Oom,
        };
        page.set_flag(PageFlags::UPTODATE);

        let phys = page_to_pfn(page) * PAGE_SIZE;
        let copied = unsafe {
            let dst = (ctx.direct_map_offset + phys) as *mut u8;
            let dst_slice = core::slice::from_raw_parts_mut(dst, PAGE_SIZE as usize);
            match fs::mmap_copy_page(backing_id, file_offset, dst_slice) {
                Ok(n) => n,
                Err(_) => {
                    free_page(page);
                    return FaultResult::Sigbus;
                }
            }
        };
        if copied == 0 {
            unsafe {
                free_page(page);
            }
            return FaultResult::Sigbus;
        }

        unsafe {
            let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);
            if !pt_mgr.map_page(address, phys, ctx.vma_flags.to_user_pte_flags()) {
                free_page(page);
                return FaultResult::Oom;
            }
        }

        page.inc_mapcount();
        FAULT_STATS.minor_faults.fetch_add(1, Ordering::Relaxed);
        return FaultResult::Retry;
    }

    // 过渡实现：若当前进程记录了该虚拟页的 exec 映射，优先复用其物理页重建 PTE。
    // 这可以覆盖“已知文件后备页”的缺页场景；完整的 VFS/page cache 路径后续再接入。
    if let Some(mapped) = crate::task::lookup_current_exec_mapping(address) {
        if mapped.phys == 0 || (mapped.phys & (PAGE_SIZE - 1)) != 0 {
            return FaultResult::Sigbus;
        }

        let pte_flags = if mapped.flags != 0 {
            mapped.flags
        } else {
            ctx.vma_flags.to_user_pte_flags()
        };

        unsafe {
            let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);
            if !pt_mgr.map_page(address, mapped.phys, pte_flags) {
                return FaultResult::Oom;
            }
        }

        let page = unsafe { &*pfn_to_page(mapped.phys / PAGE_SIZE) };
        page.inc_mapcount();
        FAULT_STATS.minor_faults.fetch_add(1, Ordering::Relaxed);
        return FaultResult::Retry;
    }

    // 尚未接入统一 VFS/page cache 时，不允许静默降级为零页。
    // 文件后备缺页必须显式失败，避免语义错误被掩盖。
    FaultResult::Sigbus
}

/// 处理写保护错误 (Copy-on-Write)
fn handle_write_protection(ctx: &FaultContext) -> FaultResult {
    // 检查是否允许写
    if !ctx.vma_flags.contains(VmFlags::MAYWRITE) {
        return FaultResult::Sigsegv;
    }

    let address = ctx.address & !(PAGE_SIZE - 1);

    unsafe {
        let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);
        let old_phys = match pt_mgr.translate_addr(address) {
            Some(p) => p & !(PAGE_SIZE - 1),
            None => return FaultResult::Sigsegv,
        };

        let old_page = &*pfn_to_page(old_phys / PAGE_SIZE);

        // 如果只有一个引用，直接修改权限
        if old_page.refcount() == 1 {
            let pte_flags = ctx.vma_flags.to_user_pte_flags();
            if !pt_mgr.map_page(address, old_phys, pte_flags) {
                return FaultResult::Oom;
            }
            return FaultResult::Retry;
        }
    }

    // 否则执行 COW
    do_cow_fault(ctx, address)
}

/// 执行 Copy-on-Write
fn do_cow_fault(ctx: &FaultContext, address: u64) -> FaultResult {
    // 1. 分配新页
    let new_page = match alloc_page(GfpFlags::new(GfpFlags::USER)) {
        Some(p) => p,
        None => return FaultResult::Oom,
    };

    let new_phys = page_to_pfn(new_page) * PAGE_SIZE;

    unsafe {
        let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);

        // 2. 复制内容
        let old_phys = match pt_mgr.translate_addr(address) {
            Some(p) => p & !(PAGE_SIZE - 1),
            None => {
                free_page(new_page);
                return FaultResult::Sigsegv;
            }
        };

        // 需要虚拟地址进行复制
        let old_virt = pt_mgr.phys_to_virt(old_phys);
        let new_virt = pt_mgr.phys_to_virt(new_phys);

        copy_page(new_virt, old_virt);

        // 3. 更新映射
        let pte_flags = ctx.vma_flags.to_user_pte_flags();
        if !pt_mgr.map_page(address, new_phys, pte_flags) {
            free_page(new_page);
            return FaultResult::Oom;
        }

        // 4. 旧页引用释放由 map_page 的重映射语义统一处理

        // 新页已映射，增加 mapcount
        new_page.inc_mapcount();
    }

    FAULT_STATS.cow_faults.fetch_add(1, Ordering::Relaxed);
    FAULT_STATS.minor_faults.fetch_add(1, Ordering::Relaxed);

    FaultResult::Retry
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 复制页面内容
#[allow(dead_code)]
unsafe fn copy_page(dst: u64, src: u64) {
    let dst_ptr = dst as *mut u8;
    let src_ptr = src as *const u8;
    core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE as usize);
}

/// 清零页面
#[allow(dead_code)]
unsafe fn zero_page(addr: u64) {
    let ptr = addr as *mut u8;
    core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
}

// ============================================================================
// 统计信息
// ============================================================================

/// 页错误统计
pub struct FaultStats {
    /// 总页错误次数
    pub total_faults: AtomicU64,
    /// 次要页错误 (无 IO)
    pub minor_faults: AtomicU64,
    /// 主要页错误 (需要 IO)
    pub major_faults: AtomicU64,
    /// COW 次数
    pub cow_faults: AtomicU64,
    /// 栈扩展次数
    pub stack_grows: AtomicU64,
}

static FAULT_STATS: FaultStats = FaultStats {
    total_faults: AtomicU64::new(0),
    minor_faults: AtomicU64::new(0),
    major_faults: AtomicU64::new(0),
    cow_faults: AtomicU64::new(0),
    stack_grows: AtomicU64::new(0),
};

/// 获取页错误统计
pub fn get_fault_stats() -> &'static FaultStats {
    &FAULT_STATS
}
