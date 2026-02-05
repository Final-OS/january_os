// ============================================================================
// january_os - x86_64 页表管理模块
// 
// 实现 x86_64 四级页表管理 (PML4 -> PDPT -> PD -> PT)
// ============================================================================

use crate::config;
use crate::mm::buddy::alloc_page;
use crate::mm::zone::GFP_KERNEL_ZERO;
use crate::mm::page::page_to_pfn;

// ============================================================================
// 页表条目标志位 (x86_64 特定)
// ============================================================================

/// 页表条目存在位
pub const PTE_PRESENT: u64 = 1 << 0;
/// 页表条目可写位
pub const PTE_WRITABLE: u64 = 1 << 1;
/// 用户态可访问位
pub const PTE_USER: u64 = 1 << 2;
/// 直写模式 (Write-Through)
pub const PTE_WRITE_THROUGH: u64 = 1 << 3;
/// 禁止缓存
pub const PTE_NO_CACHE: u64 = 1 << 4;
/// 已访问位
pub const PTE_ACCESSED: u64 = 1 << 5;
/// 已修改位 (脏位)
pub const PTE_DIRTY: u64 = 1 << 6;
/// 大页面标志 (2MB 或 1GB)
pub const PTE_HUGE: u64 = 1 << 7;
/// 全局页面 (不随 CR3 切换而刷新 TLB)
pub const PTE_GLOBAL: u64 = 1 << 8;
/// 禁止执行 (需要 NX 位支持)
pub const PTE_NO_EXECUTE: u64 = 1 << 63;

/// 地址掩码 (提取页帧地址)
pub const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

// ============================================================================
// 页表层级
// ============================================================================

/// 页表层级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableLevel {
    /// PML4 (Page Map Level 4) - 512 GB 每项
    Pml4 = 4,
    /// PDPT (Page Directory Pointer Table) - 1 GB 每项
    Pdpt = 3,
    /// PD (Page Directory) - 2 MB 每项
    Pd = 2,
    /// PT (Page Table) - 4 KB 每项
    Pt = 1,
}

impl PageTableLevel {
    /// 获取每个条目覆盖的地址范围大小
    pub const fn entry_size(&self) -> u64 {
        match self {
            PageTableLevel::Pml4 => 512 * 1024 * 1024 * 1024, // 512 GB
            PageTableLevel::Pdpt => 1024 * 1024 * 1024,        // 1 GB
            PageTableLevel::Pd => 2 * 1024 * 1024,             // 2 MB
            PageTableLevel::Pt => config::PAGE_SIZE,           // 4 KB
        }
    }
    
    /// 获取下一级页表层级
    pub const fn next_level(&self) -> Option<PageTableLevel> {
        match self {
            PageTableLevel::Pml4 => Some(PageTableLevel::Pdpt),
            PageTableLevel::Pdpt => Some(PageTableLevel::Pd),
            PageTableLevel::Pd => Some(PageTableLevel::Pt),
            PageTableLevel::Pt => None,
        }
    }
}

// ============================================================================
// 页表条目
// ============================================================================

/// 页表条目
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// 创建空条目
    pub const fn empty() -> Self {
        Self(0)
    }
    
    /// 创建新条目
    pub const fn new(phys_addr: u64, flags: u64) -> Self {
        Self((phys_addr & PTE_ADDR_MASK) | flags)
    }
    
    /// 获取原始值
    pub const fn raw(&self) -> u64 {
        self.0
    }
    
    /// 检查是否存在
    pub const fn is_present(&self) -> bool {
        self.0 & PTE_PRESENT != 0
    }
    
    /// 检查是否可写
    pub const fn is_writable(&self) -> bool {
        self.0 & PTE_WRITABLE != 0
    }
    
    /// 检查是否为用户态
    pub const fn is_user(&self) -> bool {
        self.0 & PTE_USER != 0
    }
    
    /// 检查是否为大页面
    pub const fn is_huge(&self) -> bool {
        self.0 & PTE_HUGE != 0
    }
    
    /// 检查是否禁止执行
    pub const fn is_no_execute(&self) -> bool {
        self.0 & PTE_NO_EXECUTE != 0
    }
    
    /// 获取物理地址
    pub const fn phys_addr(&self) -> u64 {
        self.0 & PTE_ADDR_MASK
    }
    
    /// 获取标志位
    pub const fn flags(&self) -> u64 {
        self.0 & !PTE_ADDR_MASK
    }
    
    /// 设置存在位
    pub fn set_present(&mut self, present: bool) {
        if present {
            self.0 |= PTE_PRESENT;
        } else {
            self.0 &= !PTE_PRESENT;
        }
    }
    
    /// 设置可写位
    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.0 |= PTE_WRITABLE;
        } else {
            self.0 &= !PTE_WRITABLE;
        }
    }
    
    /// 设置用户态可访问位
    pub fn set_user(&mut self, user: bool) {
        if user {
            self.0 |= PTE_USER;
        } else {
            self.0 &= !PTE_USER;
        }
    }
    
    /// 设置物理地址
    pub fn set_phys_addr(&mut self, addr: u64) {
        self.0 = (self.0 & !PTE_ADDR_MASK) | (addr & PTE_ADDR_MASK);
    }
}

impl core::fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PTE({:#018x} | ", self.phys_addr())?;
        if self.is_present() { write!(f, "P")?; } else { write!(f, "-")?; }
        if self.is_writable() { write!(f, "W")?; } else { write!(f, "R")?; }
        if self.is_user() { write!(f, "U")?; } else { write!(f, "S")?; }
        if self.is_huge() { write!(f, "H")?; } else { write!(f, "-")?; }
        if self.is_no_execute() { write!(f, "X")?; } else { write!(f, "-")?; }
        write!(f, ")")
    }
}

// ============================================================================
// 页表
// ============================================================================

/// 页表 (512 条目，占 4KB)
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    /// 创建空页表
    pub const fn empty() -> Self {
        const EMPTY: PageTableEntry = PageTableEntry::empty();
        Self { entries: [EMPTY; 512] }
    }
    
    /// 获取条目引用
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }
    
    /// 获取条目可变引用
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }
    
    /// 获取所有条目
    pub fn entries(&self) -> &[PageTableEntry; 512] {
        &self.entries
    }
    
    /// 获取所有条目可变引用
    pub fn entries_mut(&mut self) -> &mut [PageTableEntry; 512] {
        &mut self.entries
    }
    
    /// 清空页表
    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = PageTableEntry::empty();
        }
    }
}

// ============================================================================
// 地址索引计算 (x86_64 特定)
// ============================================================================

/// 从虚拟地址提取 PML4 索引
#[inline]
pub const fn pml4_index(virt: u64) -> usize {
    ((virt >> 39) & 0x1FF) as usize
}

/// 从虚拟地址提取 PDPT 索引
#[inline]
pub const fn pdpt_index(virt: u64) -> usize {
    ((virt >> 30) & 0x1FF) as usize
}

/// 从虚拟地址提取 PD 索引
#[inline]
pub const fn pd_index(virt: u64) -> usize {
    ((virt >> 21) & 0x1FF) as usize
}

/// 从虚拟地址提取 PT 索引
#[inline]
pub const fn pt_index(virt: u64) -> usize {
    ((virt >> 12) & 0x1FF) as usize
}

/// 从虚拟地址提取页内偏移
#[inline]
pub const fn page_offset(virt: u64) -> usize {
    (virt & 0xFFF) as usize
}

// ============================================================================
// 页表管理器
// ============================================================================

/// 页表管理器
/// 
/// 负责管理内核页表，提供虚拟地址映射功能
pub struct PageTableManager {
    /// PML4 物理地址
    pml4_phys: u64,
    /// 直接映射偏移
    direct_map_offset: u64,
}

impl PageTableManager {
    /// 创建页表管理器
    /// 
    /// # Safety
    /// pml4_phys 必须指向有效的 PML4 页表
    pub const unsafe fn new(pml4_phys: u64, direct_map_offset: u64) -> Self {
        Self {
            pml4_phys,
            direct_map_offset,
        }
    }
    
    /// 物理地址转虚拟地址（通过直接映射）
    #[inline]
    pub const fn phys_to_virt(&self, phys: u64) -> u64 {
        self.direct_map_offset + phys
    }
    
    /// 虚拟地址转物理地址（通过直接映射，仅对直接映射区有效）
    #[inline]
    pub const fn virt_to_phys(&self, virt: u64) -> u64 {
        virt - self.direct_map_offset
    }
    
    /// 获取 PML4 页表
    pub fn pml4(&self) -> &PageTable {
        unsafe {
            &*(self.phys_to_virt(self.pml4_phys) as *const PageTable)
        }
    }
    
    /// 获取 PML4 页表可变引用
    pub fn pml4_mut(&mut self) -> &mut PageTable {
        unsafe {
            &mut *(self.phys_to_virt(self.pml4_phys) as *mut PageTable)
        }
    }
    
    /// 获取 PML4 物理地址
    pub const fn pml4_phys(&self) -> u64 {
        self.pml4_phys
    }
    
    /// 遍历页表，查找虚拟地址对应的页表条目
    /// 
    /// 返回 (条目, 页表层级, 页面大小)
    pub fn translate(&self, virt_addr: u64) -> Option<(PageTableEntry, PageTableLevel, u64)> {
        let pml4 = self.pml4();
        let pml4_entry = pml4.entry(pml4_index(virt_addr));
        
        if !pml4_entry.is_present() {
            return None;
        }
        
        // PDPT
        let pdpt = unsafe {
            &*(self.phys_to_virt(pml4_entry.phys_addr()) as *const PageTable)
        };
        let pdpt_entry = pdpt.entry(pdpt_index(virt_addr));
        
        if !pdpt_entry.is_present() {
            return None;
        }
        
        // 1GB 大页面?
        if pdpt_entry.is_huge() {
            return Some((*pdpt_entry, PageTableLevel::Pdpt, 1024 * 1024 * 1024));
        }
        
        // PD
        let pd = unsafe {
            &*(self.phys_to_virt(pdpt_entry.phys_addr()) as *const PageTable)
        };
        let pd_entry = pd.entry(pd_index(virt_addr));
        
        if !pd_entry.is_present() {
            return None;
        }
        
        // 2MB 大页面?
        if pd_entry.is_huge() {
            return Some((*pd_entry, PageTableLevel::Pd, 2 * 1024 * 1024));
        }
        
        // PT
        let pt = unsafe {
            &*(self.phys_to_virt(pd_entry.phys_addr()) as *const PageTable)
        };
        let pt_entry = pt.entry(pt_index(virt_addr));
        
        if !pt_entry.is_present() {
            return None;
        }
        
        Some((*pt_entry, PageTableLevel::Pt, config::PAGE_SIZE))
    }
    
    /// 将虚拟地址转换为物理地址
    pub fn translate_addr(&self, virt_addr: u64) -> Option<u64> {
        let (entry, _level, page_size) = self.translate(virt_addr)?;
        let offset_mask = page_size - 1;
        Some(entry.phys_addr() + (virt_addr & offset_mask))
    }

    /// 刷新单个 TLB 条目
    pub fn flush_tlb(&self, virt_addr: u64) {
        unsafe {
            core::arch::asm!(
                "invlpg [{}]",
                in(reg) virt_addr,
                options(nostack, preserves_flags)
            );
        }
    }
    
    /// 刷新整个 TLB (重新加载 CR3)
    pub fn flush_tlb_all(&self) {
        unsafe {
            core::arch::asm!(
                "mov {tmp}, cr3",
                "mov cr3, {tmp}",
                tmp = out(reg) _,
                options(nostack, preserves_flags)
            );
        }
    }

    /// 映射虚拟地址到物理地址
    /// 
    /// # Safety
    /// 
    /// - 需要确保分配内存成功
    pub unsafe fn map_page(&mut self, virt: u64, phys: u64, flags: u64) -> bool {
        let direct_map_offset = self.direct_map_offset;
        let pml4_phys = self.pml4_phys;
        
        let phys_to_virt = |p: u64| direct_map_offset + p;

        let pml4 = &mut *(phys_to_virt(pml4_phys) as *mut PageTable);
        let pml4_idx = pml4_index(virt);
        let pml4_entry = &mut pml4.entries[pml4_idx];
        
        if !pml4_entry.is_present() {
            let page = match alloc_page(GFP_KERNEL_ZERO) {
                Some(p) => p,
                None => {
                    return false;
                },
            };
            let pfn = page_to_pfn(page);
            let table_phys = pfn * config::PAGE_SIZE;
            
            pml4_entry.set_phys_addr(table_phys);
            pml4_entry.set_present(true);
            pml4_entry.set_writable(true);
            pml4_entry.set_user(true); 
        }
        
        let pdpt = &mut *(phys_to_virt(pml4_entry.phys_addr()) as *mut PageTable);
        let pdpt_idx = pdpt_index(virt);
        let pdpt_entry = &mut pdpt.entries[pdpt_idx];
        
        if !pdpt_entry.is_present() {
            let page = match alloc_page(GFP_KERNEL_ZERO) {
                Some(p) => p,
                None => {
                    return false;
                },
            };
            let pfn = page_to_pfn(page);
            let table_phys = pfn * config::PAGE_SIZE;
            
            pdpt_entry.set_phys_addr(table_phys);
            pdpt_entry.set_present(true);
            pdpt_entry.set_writable(true);
            pdpt_entry.set_user(true);
        }
        
        let pd = &mut *(phys_to_virt(pdpt_entry.phys_addr()) as *mut PageTable);
        let pd_idx = pd_index(virt);
        let pd_entry = &mut pd.entries[pd_idx];
        
        if !pd_entry.is_present() {
            let page = match alloc_page(GFP_KERNEL_ZERO) {
                Some(p) => p,
                None => {
                    return false;
                },
            };
            let pfn = page_to_pfn(page);
            let table_phys = pfn * config::PAGE_SIZE;
            
            pd_entry.set_phys_addr(table_phys);
            pd_entry.set_present(true);
            pd_entry.set_writable(true);
            pd_entry.set_user(true);
        }
        
        let pt = &mut *(phys_to_virt(pd_entry.phys_addr()) as *mut PageTable);
        let pt_idx = pt_index(virt);
        let pt_entry = &mut pt.entries[pt_idx];
        
        *pt_entry = PageTableEntry::new(phys, flags);
        
        self.flush_tlb(virt);
        
        true
    }

    /// 取消映射虚拟地址
    /// 
    /// 成功返回 true，如果未映射返回 false
    pub unsafe fn unmap_page(&mut self, virt: u64) -> bool {
        let direct_map_offset = self.direct_map_offset;
        let pml4_phys = self.pml4_phys;
        
        let phys_to_virt = |p: u64| direct_map_offset + p;

        let pml4 = &mut *(phys_to_virt(pml4_phys) as *mut PageTable);
        let pml4_idx = pml4_index(virt);
        let pml4_entry = &mut pml4.entries[pml4_idx];
        
        if !pml4_entry.is_present() {
            return false;
        }

        let pdpt = &mut *(phys_to_virt(pml4_entry.phys_addr()) as *mut PageTable);
        let pdpt_idx = pdpt_index(virt);
        let pdpt_entry = &mut pdpt.entries[pdpt_idx];

        if !pdpt_entry.is_present() {
            return false;
        }
        
        if pdpt_entry.is_huge() {
            *pdpt_entry = PageTableEntry::empty();
            self.flush_tlb(virt);
            return true;
        }

        let pd = &mut *(phys_to_virt(pdpt_entry.phys_addr()) as *mut PageTable);
        let pd_idx = pd_index(virt);
        let pd_entry = &mut pd.entries[pd_idx];

        if !pd_entry.is_present() {
            return false;
        }
        
        if pd_entry.is_huge() {
            *pd_entry = PageTableEntry::empty();
            self.flush_tlb(virt);
            return true;
        }

        let pt = &mut *(phys_to_virt(pd_entry.phys_addr()) as *mut PageTable);
        let pt_idx = pt_index(virt);
        let pt_entry = &mut pt.entries[pt_idx];

        if !pt_entry.is_present() {
            return false;
        }

        *pt_entry = PageTableEntry::empty();
        self.flush_tlb(virt);
        
        true
    }
    
    /// 获取 PML4 中存在的条目数量
    pub fn count_pml4_entries(&self) -> usize {
        self.pml4().entries().iter()
            .filter(|e| e.is_present())
            .count()
    }
}
