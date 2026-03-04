// ============================================================================
// january_os - x86_64 TLB (Translation Lookaside Buffer) 管理
// ============================================================================

/// 刷新单个 TLB 条目
#[inline]
pub fn flush_tlb(virt_addr: u64) {
    unsafe {
        core::arch::asm!(
            "invlpg [{}]",
            in(reg) virt_addr,
            options(nostack, preserves_flags)
        );
    }
}

/// 刷新整个 TLB (重新加载 CR3)
#[inline]
pub fn flush_tlb_all() {
    unsafe {
        core::arch::asm!(
            "mov {tmp}, cr3",
            "mov cr3, {tmp}",
            tmp = out(reg) _,
            options(nostack, preserves_flags)
        );
    }
}

/// 刷新指定范围的 TLB 条目
#[inline]
pub fn flush_tlb_range(start: u64, end: u64, page_size: u64) {
    let mut addr = start;
    while addr < end {
        flush_tlb(addr);
        addr += page_size;
    }
}

/// 获取当前 CR3 值
#[inline]
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) cr3,
            options(nostack, preserves_flags)
        );
    }
    cr3
}

/// 获取当前 CR4 值
#[inline]
pub fn read_cr4() -> u64 {
    let cr4: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) cr4,
            options(nostack, preserves_flags)
        );
    }
    cr4
}

#[derive(Clone, Copy, Debug)]
pub struct PagingHardwareState {
    pub cr3_root: u64,
    pub cr4: u64,
    pub la57_active: bool,
    pub page_levels: u8,
    pub va_bits: u8,
}

#[inline]
pub fn paging_hardware_state() -> PagingHardwareState {
    let cr3_root = read_cr3() & 0x000F_FFFF_FFFF_F000;
    let cr4 = read_cr4();
    let la57_active = (cr4 & (1 << 12)) != 0;
    PagingHardwareState {
        cr3_root,
        cr4,
        la57_active,
        page_levels: if la57_active { 5 } else { 4 },
        va_bits: if la57_active { 57 } else { 48 },
    }
}

/// 设置 CR3 值 (切换页表)
/// 
/// # Safety
/// 必须确保新的 CR3 值指向有效的页表
#[inline]
pub unsafe fn write_cr3(cr3: u64) {
    core::arch::asm!(
        "mov cr3, {}",
        in(reg) cr3,
        options(nostack, preserves_flags)
    );
}

/// 启用/禁用全局页面 (CR4.PGE)
#[inline]
pub fn set_global_pages_enabled(enabled: bool) {
    unsafe {
        let mut cr4: u64;
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) cr4,
            options(nostack, preserves_flags)
        );
        
        if enabled {
            cr4 |= 1 << 7; // CR4.PGE
        } else {
            cr4 &= !(1 << 7);
        }
        
        core::arch::asm!(
            "mov cr4, {}",
            in(reg) cr4,
            options(nostack, preserves_flags)
        );
    }
}

/// 读取 CR2 (页错误地址)
#[inline]
pub fn read_cr2() -> u64 {
    let cr2: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr2",
            out(reg) cr2,
            options(nostack, preserves_flags)
        );
    }
    cr2
}
