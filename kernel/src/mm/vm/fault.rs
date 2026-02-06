// ============================================================================
// january_os - 页错误处理
//
// 处理页错误异常，实现 demand paging 和 COW
// ============================================================================

use super::layout::PAGE_SIZE;
use super::vma::{Vma, VmFlags, Mm};
use crate::mm::page::page::{Page, PageFlags, pfn_to_page, page_to_pfn};
use crate::mm::page::zone::GfpFlags;
use crate::mm::page::buddy::{alloc_page, free_page};
use super::paging::PageTableManager;

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
    pub const WRITE: u64   = 1 << 1;
    /// 用户态 (vs 内核态)
    pub const USER: u64    = 1 << 2;
    /// 保留位被设置
    pub const RSVD: u64    = 1 << 3;
    /// 取指访问
    pub const INSTR: u64   = 1 << 4;
    /// 保护密钥违规
    pub const PK: u64      = 1 << 5;
    /// 影子栈访问
    pub const SS: u64      = 1 << 6;
    
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
    /// 相关 VMA
    pub vma: *mut Vma,
    /// 直接映射偏移
    pub direct_map_offset: u64,
}

impl FaultContext {
    pub fn new(address: u64, error_code: u64, mm: *mut Mm, direct_map: u64) -> Self {
        Self {
            address,
            error_code: PageFaultError::new(error_code),
            mm,
            vma: core::ptr::null_mut(),
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
        let vma = match mm.find_vma_mut(address) {
            Some(v) => v,
            None => {
                // 检查是否是栈扩展
                if can_expand_stack(mm, address) {
                    return handle_stack_expansion(ctx, mm, address);
                }
                return FaultResult::Sigsegv;
            }
        };
        
        ctx.vma = vma;
        
        // 4. 检查权限
        if !check_access_permissions(vma, &error) {
            return FaultResult::Sigsegv;
        }
        
        // 5. 处理具体错误类型
        if !error.is_protection_violation() {
            // 页不存在
            return handle_not_present(ctx, vma);
        } else if error.is_write() {
            // 写保护，可能是 COW
            return handle_write_protection(ctx, vma);
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
    if address >= crate::mm::vmalloc::VMALLOC_START && address < crate::mm::vmalloc::VMALLOC_END {
        // vmalloc 区域的页错误通常是 bug
        return FaultResult::KernelOops;
    }
    
    // 直接映射区域
    let direct_map_start = ctx.direct_map_offset;
    let direct_map_end = direct_map_start + 0x100_0000_0000; // 1TB
    if address >= direct_map_start && address < direct_map_end {
        // 直接映射应该始终存在
        return FaultResult::KernelOops;
    }
    
    // 其他内核地址
    FaultResult::KernelOops
}

/// 检查访问权限
fn check_access_permissions(vma: &Vma, error: &PageFaultError) -> bool {
    // 写访问检查
    if error.is_write() && !vma.vm_flags.is_write() {
        return false;
    }
    
    // 执行访问检查
    if error.is_instruction_fetch() && !vma.vm_flags.is_exec() {
        return false;
    }
    
    // 读访问检查
    if !error.is_write() && !error.is_instruction_fetch() && !vma.vm_flags.is_read() {
        return false;
    }
    
    true
}

// ============================================================================
// 具体错误类型处理
// ============================================================================

/// 处理页不存在错误 (demand paging)
fn handle_not_present(ctx: &FaultContext, vma: &Vma) -> FaultResult {
    if vma.vm_flags.is_anonymous() {
        // 匿名映射：分配零页
        handle_anonymous_fault(ctx, vma)
    } else {
        // 文件映射：从文件读取
        handle_file_fault(ctx, vma)
    }
}

/// 处理匿名页错误
fn handle_anonymous_fault(ctx: &FaultContext, vma: &Vma) -> FaultResult {
    let address = ctx.address & !(PAGE_SIZE - 1); // 页对齐
    
    // 分配新页
    let page = match alloc_page(GfpFlags::new(GfpFlags::USER | GfpFlags::ZERO)) {
        Some(p) => p,
        None => return FaultResult::Oom,
    };
    
    // 设置页标志
    page.set_flag(PageFlags::UPTODATE);
    if vma.vm_flags.is_anonymous() {
        page.set_flag(PageFlags::ANON);
    }
    
    // 映射页面
    let phys = page_to_pfn(page) * PAGE_SIZE;
    let pte_flags = vma.vm_flags.to_pte_flags();
    
    // 实际映射到页表
    unsafe {
        let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);
        if !pt_mgr.map_page(address, phys, pte_flags) {
            free_page(page);
            return FaultResult::Oom;
        }
    }
    
    FaultResult::Retry
}

/// 处理文件映射页错误
fn handle_file_fault(_ctx: &FaultContext, _vma: &Vma) -> FaultResult {
    // TODO: 实现文件映射
    // 1. 从 vma->vm_file 读取页面
    // 2. 映射到地址空间
    
    FaultResult::Sigsegv
}

/// 处理写保护错误 (Copy-on-Write)
fn handle_write_protection(ctx: &FaultContext, vma: &Vma) -> FaultResult {
    // 检查是否允许写
    if !vma.vm_flags.contains(VmFlags::MAYWRITE) {
        return FaultResult::Sigsegv;
    }
    
    let address = ctx.address & !(PAGE_SIZE - 1);
    
    unsafe {
        let mut pt_mgr = PageTableManager::new((*ctx.mm).pgd, ctx.direct_map_offset);
        let old_phys = match pt_mgr.translate_addr(address) {
            Some(p) => p & !(PAGE_SIZE - 1),
            None => return FaultResult::Sigsegv,
        };
        
        let old_page = pfn_to_page(old_phys / PAGE_SIZE);
        
        // 如果只有一个引用，直接修改权限
        if old_page.refcount() == 1 {
            let pte_flags = vma.vm_flags.to_pte_flags();
            pt_mgr.map_page(address, old_phys, pte_flags);
            return FaultResult::Retry;
        }
    }
    
    // 否则执行 COW
    do_cow_fault(ctx, vma, address)
}

/// 执行 Copy-on-Write
fn do_cow_fault(ctx: &FaultContext, vma: &Vma, address: u64) -> FaultResult {
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
        let pte_flags = vma.vm_flags.to_pte_flags();
        pt_mgr.map_page(address, new_phys, pte_flags);
        
        // 4. 减少旧页引用
        let old_page = pfn_to_page(old_phys / PAGE_SIZE);
        if old_page.put() == 0 {
            free_page(old_page);
        }
    }
    
    FaultResult::Retry
}

// ============================================================================
// 栈扩展
// ============================================================================

/// 检查是否可以扩展栈
fn can_expand_stack(mm: &Mm, address: u64) -> bool {
    // 栈向下增长，检查是否在栈底附近
    let stack_bottom = mm.start_stack.saturating_sub(mm.stack_vm * PAGE_SIZE);
    
    // 允许栈扩展的最大距离 (如 256 KB)
    const MAX_STACK_EXPAND: u64 = 256 * 1024;
    
    address >= stack_bottom.saturating_sub(MAX_STACK_EXPAND) && address < stack_bottom
}

/// 处理栈扩展
fn handle_stack_expansion(ctx: &FaultContext, mm: &mut Mm, address: u64) -> FaultResult {
    let page_addr = address & !(PAGE_SIZE - 1);
    
    // 查找栈 VMA
    let stack_vma: Option<*mut Vma> = unsafe {
        let mut node = mm.vma_list.next;
        let head = &mm.vma_list as *const _ as *mut crate::mm::page::ListHead;
        let mut found: Option<*mut Vma> = None;
        
        while node != head {
            let vma = super::vma::container_of!(node, Vma, vm_list);
            if (*vma).vm_flags.contains(VmFlags::GROWSDOWN) {
                // 检查是否是我们的栈
                if page_addr < (*vma).vm_start && page_addr >= (*vma).vm_start.saturating_sub(256 * 1024) {
                    found = Some(vma);
                    break;
                }
            }
            node = (*node).next;
        }
        found
    };
    
    match stack_vma {
        Some(vma_ptr) => unsafe {
            let vma = &mut *vma_ptr;
            // 扩展栈
            let old_start = vma.vm_start;
            vma.vm_start = page_addr;
            mm.stack_vm += (old_start - page_addr) / PAGE_SIZE;
            mm.total_vm += (old_start - page_addr) / PAGE_SIZE;
            
            // 分配并映射新页
            handle_anonymous_fault(ctx, vma)
        }
        None => FaultResult::Sigsegv,
    }
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
    pub total_faults: u64,
    /// 次要页错误 (无 IO)
    pub minor_faults: u64,
    /// 主要页错误 (需要 IO)
    pub major_faults: u64,
    /// COW 次数
    pub cow_faults: u64,
    /// 栈扩展次数
    pub stack_grows: u64,
}

static mut FAULT_STATS: FaultStats = FaultStats {
    total_faults: 0,
    minor_faults: 0,
    major_faults: 0,
    cow_faults: 0,
    stack_grows: 0,
};

/// 获取页错误统计
pub fn get_fault_stats() -> &'static FaultStats {
    unsafe { &*core::ptr::addr_of!(FAULT_STATS) }
}
