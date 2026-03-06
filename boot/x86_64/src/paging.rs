//! 引导页表与内存映射转换

use core::arch::{asm, global_asm};

use uefi::boot::MemoryType;

use crate::bootinfo::{
    DIRECT_MAP_OFFSET, KERNEL_PHYS_ADDR, KERNEL_VIRT_ADDR, MAX_MEMORY_REGIONS, MemoryRegion,
    MemoryRegionType, PAGE_TABLE_BUFFER_PAGES,
};
use crate::cfg::{KERNEL_LA57_FALLBACK, KERNEL_VA_MODE, VMALLOC_START};

/// 页大小 4KB
pub const PAGE_SIZE: u64 = 4096;
/// 1 GiB
const ONE_GIB: u64 = 0x4000_0000;
/// 页表条目数量
const ENTRIES_PER_TABLE: usize = 512;
/// 页表地址掩码
const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
/// 页表项标志：存在
const PTE_PRESENT: u64 = 1 << 0;
/// 页表项标志：可写
const PTE_WRITABLE: u64 = 1 << 1;
/// 页表项标志：大页 (2MB/1GB)
const PTE_HUGE: u64 = 1 << 7;
/// 页表项标志：全局
const PTE_GLOBAL: u64 = 1 << 8;
/// CR4.LA57
const CR4_LA57: u64 = 1 << 12;
/// 启动期 4->5 切换 trampoline 开关。
const ENABLE_LA57_TRAMPOLINE: bool = true;

#[derive(Clone, Copy)]
pub struct PagingModeProbe {
    pub cpuid_7_0_ecx: u32,
    pub cr4: u64,
    pub la57_supported: bool,
    pub la57_active: bool,
    pub va_mode_la57_prefer: bool,
    pub fallback_4level: bool,
    pub transition_requested: bool,
    pub current_page_levels: u8,
    pub current_va_bits: u8,
    pub target_page_levels: u8,
    pub target_va_bits: u8,
}

/// 运行时页表布局输出
#[derive(Clone, Copy)]
pub struct PageTableSetupResult {
    /// CR3 应加载的根页表物理地址
    pub root_phys_addr: u64,
    /// 兼容字段（4-level 下等于 root；5-level 下为内核高半区对应 PML4）
    pub pml4_compat_phys: u64,
    /// 实际页表层级（4 或 5）
    pub page_levels: u8,
    /// 规范地址位宽（48 或 57）
    pub va_bits: u8,
    /// direct-map 虚拟窗口结束地址（exclusive）
    pub direct_map_window_end: u64,
    /// 若非 0，表示应在 handoff 阶段尝试 4->5 级切换（值为 5-level root）
    pub la57_transition_root_phys: u64,
    /// 5-level 兼容 PML4（仅在 `la57_transition_root_phys != 0` 时有效）
    pub la57_transition_pml4_compat_phys: u64,
    /// 4-level fallback root（仅在 `la57_transition_root_phys != 0` 时有效）
    pub fallback_root_phys: u64,
}

/// 页表分配器状态
struct PageTableAllocator {
    next_page: u64,
    end_page: u64,
}

impl PageTableAllocator {
    fn new(start: u64, pages: usize) -> Self {
        Self {
            next_page: start,
            end_page: start + (pages as u64) * PAGE_SIZE,
        }
    }

    /// 分配一个零初始化的页面
    unsafe fn alloc_page(&mut self) -> Option<u64> {
        if self.next_page + PAGE_SIZE > self.end_page {
            return None;
        }
        let page = self.next_page;
        self.next_page += PAGE_SIZE;
        unsafe { core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize) };
        Some(page)
    }
}

#[inline]
const fn level_index(virt: u64, level: u8) -> usize {
    let shift = 12 + (level - 1) * 9;
    ((virt >> shift) & 0x1ff) as usize
}

#[inline]
unsafe fn table_entry_mut(table_phys: u64, idx: usize) -> *mut u64 {
    unsafe { (table_phys as *mut u64).add(idx) }
}

#[inline]
unsafe fn ensure_next_table(
    allocator: &mut PageTableAllocator,
    table_phys: u64,
    idx: usize,
) -> Option<u64> {
    let entry_ptr = unsafe { table_entry_mut(table_phys, idx) };
    let entry = unsafe { *entry_ptr };
    if entry & PTE_PRESENT != 0 {
        return Some(entry & PTE_ADDR_MASK);
    }

    let child = unsafe { allocator.alloc_page()? };
    unsafe { *entry_ptr = child | PTE_PRESENT | PTE_WRITABLE };
    Some(child)
}

unsafe fn map_1g_page(
    allocator: &mut PageTableAllocator,
    root_phys: u64,
    page_levels: u8,
    virt: u64,
    phys: u64,
) -> bool {
    let mut table_phys = root_phys;
    for level in (4..=page_levels).rev() {
        let idx = level_index(virt, level);
        let Some(next) = (unsafe { ensure_next_table(allocator, table_phys, idx) }) else {
            return false;
        };
        table_phys = next;
    }

    let pdpt_idx = level_index(virt, 3);
    let pdpt_entry = unsafe { table_entry_mut(table_phys, pdpt_idx) };
    unsafe {
        *pdpt_entry = (phys & PTE_ADDR_MASK) | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | PTE_GLOBAL
    };
    true
}

unsafe fn map_4k_page(
    allocator: &mut PageTableAllocator,
    root_phys: u64,
    page_levels: u8,
    virt: u64,
    phys: u64,
    flags: u64,
) -> bool {
    let mut table_phys = root_phys;
    for level in (2..=page_levels).rev() {
        let idx = level_index(virt, level);
        let Some(next) = (unsafe { ensure_next_table(allocator, table_phys, idx) }) else {
            return false;
        };
        table_phys = next;
    }

    let pt_idx = level_index(virt, 1);
    let pte = unsafe { table_entry_mut(table_phys, pt_idx) };
    unsafe { *pte = (phys & PTE_ADDR_MASK) | flags };
    true
}

unsafe fn kernel_pml4_from_root(root_phys: u64, page_levels: u8) -> u64 {
    if page_levels == 4 {
        return root_phys;
    }
    let pml5_idx = level_index(KERNEL_VIRT_ADDR, 5);
    let pml5e = unsafe { *(root_phys as *const u64).add(pml5_idx) };
    if pml5e & PTE_PRESENT == 0 {
        0
    } else {
        pml5e & PTE_ADDR_MASK
    }
}

unsafe fn setup_page_tables_nlevel(
    allocator: &mut PageTableAllocator,
    kernel_size: u64,
    max_phys_addr: u64,
    page_levels: u8,
    va_bits: u8,
) -> PageTableSetupResult {
    let Some(root) = (unsafe { allocator.alloc_page() }) else {
        return PageTableSetupResult {
            root_phys_addr: 0,
            pml4_compat_phys: 0,
            page_levels,
            va_bits,
            direct_map_window_end: VMALLOC_START,
            la57_transition_root_phys: 0,
            la57_transition_pml4_compat_phys: 0,
            fallback_root_phys: 0,
        };
    };

    // 1) 恒等映射前 4GiB（4 x 1GiB）
    for i in 0..4u64 {
        let addr = i * ONE_GIB;
        if !unsafe { map_1g_page(allocator, root, page_levels, addr, addr) } {
            return PageTableSetupResult {
                root_phys_addr: 0,
                pml4_compat_phys: 0,
                page_levels,
                va_bits,
                direct_map_window_end: VMALLOC_START,
                la57_transition_root_phys: 0,
                la57_transition_pml4_compat_phys: 0,
                fallback_root_phys: 0,
            };
        }
    }

    // 2) 内核高半区 bootstrap 映射：
    //    - 先把 kernel base 对齐到 2MiB 窗口并 identity-map 前 2MiB；
    //    - 再覆盖内核真实文件页映射。
    let kernel_window_base = KERNEL_VIRT_ADDR & !((2 * 1024 * 1024) - 1);
    for i in 0..ENTRIES_PER_TABLE as u64 {
        let virt = kernel_window_base + i * PAGE_SIZE;
        let phys = i * PAGE_SIZE;
        if !unsafe {
            map_4k_page(
                allocator,
                root,
                page_levels,
                virt,
                phys,
                PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL,
            )
        } {
            return PageTableSetupResult {
                root_phys_addr: 0,
                pml4_compat_phys: 0,
                page_levels,
                va_bits,
                direct_map_window_end: VMALLOC_START,
                la57_transition_root_phys: 0,
                la57_transition_pml4_compat_phys: 0,
                fallback_root_phys: 0,
            };
        }
    }

    let kernel_pages = (kernel_size + PAGE_SIZE - 1) / PAGE_SIZE;
    for i in 0..kernel_pages {
        let virt = KERNEL_VIRT_ADDR + i * PAGE_SIZE;
        let phys = KERNEL_PHYS_ADDR + i * PAGE_SIZE;
        if !unsafe {
            map_4k_page(
                allocator,
                root,
                page_levels,
                virt,
                phys,
                PTE_PRESENT | PTE_WRITABLE | PTE_GLOBAL,
            )
        } {
            return PageTableSetupResult {
                root_phys_addr: 0,
                pml4_compat_phys: 0,
                page_levels,
                va_bits,
                direct_map_window_end: VMALLOC_START,
                la57_transition_root_phys: 0,
                la57_transition_pml4_compat_phys: 0,
                fallback_root_phys: 0,
            };
        }
    }

    // 3) 直接映射区（1GiB huge pages）
    let direct_map_max_span = VMALLOC_START.saturating_sub(DIRECT_MAP_OFFSET);
    let max_to_map = max_phys_addr
        .max(4 * 1024 * 1024 * 1024)
        .min(direct_map_max_span);
    let gb_pages = (max_to_map + ONE_GIB - 1) / ONE_GIB;
    for i in 0..gb_pages {
        let phys = i * ONE_GIB;
        let virt = DIRECT_MAP_OFFSET + phys;
        if !unsafe { map_1g_page(allocator, root, page_levels, virt, phys) } {
            return PageTableSetupResult {
                root_phys_addr: 0,
                pml4_compat_phys: 0,
                page_levels,
                va_bits,
                direct_map_window_end: VMALLOC_START,
                la57_transition_root_phys: 0,
                la57_transition_pml4_compat_phys: 0,
                fallback_root_phys: 0,
            };
        }
    }

    let pml4_compat_phys = unsafe { kernel_pml4_from_root(root, page_levels) };
    PageTableSetupResult {
        root_phys_addr: root,
        pml4_compat_phys,
        page_levels,
        va_bits,
        direct_map_window_end: VMALLOC_START,
        la57_transition_root_phys: 0,
        la57_transition_pml4_compat_phys: 0,
        fallback_root_phys: 0,
    }
}

unsafe fn setup_page_tables_4l(
    allocator: &mut PageTableAllocator,
    kernel_size: u64,
    max_phys_addr: u64,
) -> PageTableSetupResult {
    unsafe { setup_page_tables_nlevel(allocator, kernel_size, max_phys_addr, 4, 48) }
}

unsafe fn setup_page_tables_5l(
    allocator: &mut PageTableAllocator,
    kernel_size: u64,
    max_phys_addr: u64,
) -> PageTableSetupResult {
    unsafe { setup_page_tables_nlevel(allocator, kernel_size, max_phys_addr, 5, 57) }
}

#[inline]
pub fn probe_paging_mode() -> PagingModeProbe {
    let cpuid_leaf = core::arch::x86_64::__cpuid_count(7, 0);
    let cpuid_7_0_ecx = cpuid_leaf.ecx;
    let la57_supported = (cpuid_7_0_ecx & (1 << 16)) != 0;
    let cr4 = read_cr4();
    let la57_active = (cr4 & CR4_LA57) != 0;
    let va_mode_la57_prefer = KERNEL_VA_MODE == "la57_prefer";
    let fallback_4level = KERNEL_LA57_FALLBACK == "4level";
    let transition_requested =
        ENABLE_LA57_TRAMPOLINE && va_mode_la57_prefer && la57_supported && !la57_active;

    let (current_page_levels, current_va_bits) = if la57_active { (5, 57) } else { (4, 48) };
    let (target_page_levels, target_va_bits) = if va_mode_la57_prefer && la57_supported {
        (5, 57)
    } else {
        (4, 48)
    };

    PagingModeProbe {
        cpuid_7_0_ecx,
        cr4,
        la57_supported,
        la57_active,
        va_mode_la57_prefer,
        fallback_4level,
        transition_requested,
        current_page_levels,
        current_va_bits,
        target_page_levels,
        target_va_bits,
    }
}

/// 设置内核页表并选择 4-level / 5-level 路径。
///
/// 说明：
/// - 当前阶段仅在固件已处于 LA57 模式时启用 5-level；
/// - 若配置为 `la57_prefer` 但当前未激活 LA57，则按回退策略继续 4-level 启动。
pub unsafe fn setup_page_tables(
    kernel_size: u64,
    max_phys_addr: u64,
    page_table_start: u64,
    page_table_aux_start: u64,
) -> PageTableSetupResult {
    let probe = probe_paging_mode();
    let mut allocator = PageTableAllocator::new(page_table_start, PAGE_TABLE_BUFFER_PAGES);

    if probe.va_mode_la57_prefer && probe.la57_supported {
        if probe.la57_active {
            return unsafe { setup_page_tables_5l(&mut allocator, kernel_size, max_phys_addr) };
        }

        if ENABLE_LA57_TRAMPOLINE && probe.fallback_4level {
            let base4 = unsafe { setup_page_tables_4l(&mut allocator, kernel_size, max_phys_addr) };
            if base4.root_phys_addr == 0 {
                return base4;
            }

            let mut allocator5 =
                PageTableAllocator::new(page_table_aux_start, PAGE_TABLE_BUFFER_PAGES);
            let plan5 =
                unsafe { setup_page_tables_5l(&mut allocator5, kernel_size, max_phys_addr) };

            if plan5.root_phys_addr == 0
                || plan5.pml4_compat_phys == 0
                || (plan5.root_phys_addr >> 32) != 0
                || (page_table_aux_start >> 32) != 0
            {
                return base4;
            }

            return PageTableSetupResult {
                root_phys_addr: base4.root_phys_addr,
                pml4_compat_phys: base4.pml4_compat_phys,
                page_levels: 4,
                va_bits: 48,
                direct_map_window_end: base4.direct_map_window_end,
                la57_transition_root_phys: plan5.root_phys_addr,
                la57_transition_pml4_compat_phys: plan5.pml4_compat_phys,
                fallback_root_phys: base4.root_phys_addr,
            };
        }
    }

    // 当前固件未激活 LA57 或策略要求回退，统一走 4-level 稳定路径。
    let _ = probe.fallback_4level;
    unsafe { setup_page_tables_4l(&mut allocator, kernel_size, max_phys_addr) }
}

#[inline]
fn read_cr4() -> u64 {
    let cr4: u64;
    unsafe {
        asm!("mov {}, cr4", out(reg) cr4, options(nostack, preserves_flags));
    }
    cr4
}

unsafe extern "sysv64" {
    fn boot_enter_kernel_with_la57_fallback_start();
    fn boot_enter_kernel_with_la57_fallback_end();
}

type La57TransitionEntry = unsafe extern "sysv64" fn(u64, u64, u64, u64, u64, u64) -> !;

#[inline(never)]
unsafe fn enter_kernel_4level_fallback(
    fallback_root_phys: u64,
    kernel_stack_top: u64,
    boot_info_ptr: u64,
    kernel_entry: u64,
) -> ! {
    unsafe {
        asm!(
            "cli",
            "mov cr3, {root}",
            "mov rsp, {stack}",
            "mov rdi, {boot_info}",
            "jmp {entry}",
            root = in(reg) fallback_root_phys,
            stack = in(reg) kernel_stack_top,
            boot_info = in(reg) boot_info_ptr,
            entry = in(reg) kernel_entry,
            options(noreturn)
        );
    }
}

/// 启动期 4-level -> 5-level 切换 trampoline（失败即回退 4-level 并继续跳内核）。
///
/// # Safety
/// 调用方必须保证参数地址可用且页表内容有效。
pub unsafe fn enter_kernel_with_la57_fallback(
    la57_root_phys: u64,
    fallback_root_phys: u64,
    kernel_stack_top: u64,
    boot_info_ptr: u64,
    kernel_entry: u64,
    trampoline_phys: u64,
    trampoline_stack_top: u64,
) -> ! {
    let tramp_start = boot_enter_kernel_with_la57_fallback_start as *const () as usize;
    let tramp_end = boot_enter_kernel_with_la57_fallback_end as *const () as usize;
    let tramp_size = tramp_end.saturating_sub(tramp_start);
    let tramp_capacity = trampoline_stack_top
        .saturating_add(8)
        .saturating_sub(trampoline_phys) as usize;

    if trampoline_phys == 0
        || (trampoline_phys >> 32) != 0
        || tramp_size == 0
        || tramp_size > tramp_capacity
    {
        unsafe {
            enter_kernel_4level_fallback(
                fallback_root_phys,
                kernel_stack_top,
                boot_info_ptr,
                kernel_entry,
            )
        };
    }

    unsafe {
        core::ptr::copy_nonoverlapping(
            tramp_start as *const u8,
            trampoline_phys as *mut u8,
            tramp_size,
        );
    }

    let tramp_fn: La57TransitionEntry =
        unsafe { core::mem::transmute::<usize, La57TransitionEntry>(trampoline_phys as usize) };
    unsafe {
        tramp_fn(
            la57_root_phys,
            fallback_root_phys,
            kernel_stack_top,
            boot_info_ptr,
            kernel_entry,
            trampoline_stack_top,
        )
    }
}

global_asm!(
    r#"
.section .text
.global boot_enter_kernel_with_la57_fallback_start
.global boot_enter_kernel_with_la57_fallback
.global boot_enter_kernel_with_la57_fallback_end
boot_enter_kernel_with_la57_fallback_start:
.set LA57_GDT_LIMIT, .Lla57_gdt_end - .Lla57_gdt - 1
.set LA57_LONG64_DELTA, .Llong64 - .Lnext_eip
boot_enter_kernel_with_la57_fallback:
    cli
    mov r12, rdx
    mov r13, rcx
    mov r14, r8
    mov r15, r9
    mov r11, rsi
    mov rax, rdi
    shr rax, 32
    jnz .Lfallback_4l
    mov eax, 7
    xor ecx, ecx
    cpuid
    bt ecx, 16
    jnc .Lfallback_4l
    mov rax, cr4
    test rax, 0x1000
    jnz .Lenter_5l_direct
    mov ebx, edi
    sub rsp, 16
    mov word ptr [rsp], 0x1f
    lea rax, [rip + .Lla57_gdt]
    mov qword ptr [rsp + 2], rax
    lgdt [rsp]
    add rsp, 16
    mov esi, r15d
    lea rax, [rip + .Lcompat32]
    push 0x8
    push rax
    retfq

.Lenter_5l_direct:
    mov cr3, rdi
    mov rsp, r12
    mov rdi, r13
    jmp r14

.Lfallback_4l:
    mov cr3, r11
    mov rsp, r12
    mov rdi, r13
    jmp r14

.code32
.Lcompat32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax
    mov eax, cr0
    and eax, 0x7fffffff
    mov cr0, eax
    mov esp, esi

    mov eax, cr4
    or eax, (1 << 5) | (1 << 12)
    mov cr4, eax

    mov eax, ebx
    mov cr3, eax

    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    call .Lnext_eip
.Lnext_eip:
    pop eax
    add eax, OFFSET LA57_LONG64_DELTA
    push 0x18
    push eax
    retf

.code64
.Llong64:
    xor eax, eax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov rsp, r12
    mov rdi, r13
    jmp r14

.align 8
.Lla57_gdt:
    .quad 0x0000000000000000
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
    .quad 0x00209a0000000000
.Lla57_gdt_end:
boot_enter_kernel_with_la57_fallback_end:
"#
);

pub unsafe fn copy_memory_map<'a>(
    mmap: impl Iterator<Item = &'a uefi::mem::memory_map::MemoryDescriptor>,
    memmap_phys: u64,
) -> (u32, u64, u64, u64) {
    let dest = memmap_phys as *mut MemoryRegion;
    let mut count = 0u32;
    let mut total_mem = 0u64;
    let mut usable_mem = 0u64;
    let mut max_phys_addr = 0u64;

    for entry in mmap {
        if count >= MAX_MEMORY_REGIONS as u32 {
            break;
        }

        let pages = entry.page_count;
        let size = pages * 4096;
        total_mem += size;

        let end_addr = entry.phys_start + size;
        if end_addr > max_phys_addr {
            max_phys_addr = end_addr;
        }

        let region_type = match entry.ty {
            MemoryType::CONVENTIONAL => {
                usable_mem += size;
                MemoryRegionType::Usable
            }
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => {
                MemoryRegionType::BootloaderReclaimable
            }
            MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => {
                usable_mem += size;
                MemoryRegionType::Usable
            }
            MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => {
                MemoryRegionType::Reserved
            }
            MemoryType::ACPI_RECLAIM => MemoryRegionType::AcpiReclaimable,
            MemoryType::ACPI_NON_VOLATILE => MemoryRegionType::AcpiNvs,
            MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => MemoryRegionType::Mmio,
            _ => MemoryRegionType::Reserved,
        };

        let region = MemoryRegion {
            phys_start: entry.phys_start,
            virt_start: entry.virt_start,
            page_count: pages,
            region_type: region_type as u32,
            attributes: entry.att.bits() as u32,
        };

        unsafe { core::ptr::write_volatile(dest.add(count as usize), region) };
        count += 1;
    }

    (count, total_mem, usable_mem, max_phys_addr)
}
