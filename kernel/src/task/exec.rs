//! exec 加载规划与映射（Batch 3）
//!
//! 当前提供：
//! - ELF64 Header / Program Header 解析
//! - PT_LOAD 映射规划
//! - 最小真实映射（含回滚）
//! - 用户态入口帧参数构建所需数据

use alloc::vec::Vec;
use core::cmp;
use core::mem::size_of;

use crate::mm;
use crate::syscall::{E2BIG, EBUSY, EFAULT, EINVAL, ENOENT, ENOMEM, ESRCH};

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LSB: u8 = 1;
const ELF_VERSION_CURRENT: u8 = 1;

const PT_LOAD: u32 = 1;

const PF_X: u32 = 1;
const PF_W: u32 = 2;

const DEFAULT_USER_STACK_PAGES: u64 = if crate::config::USER_STACK_INIT_PAGES > 0 {
    crate::config::USER_STACK_INIT_PAGES
} else {
    1
};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

#[derive(Debug, Clone)]
pub struct ExecLoadSegmentPlan {
    pub vaddr: u64,
    pub mem_size: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub align: u64,
    pub page_start: u64,
    pub page_end: u64,
    pub page_count: u64,
    pub map_flags: u64,
    pub executable: bool,
    pub writable: bool,
}

#[derive(Debug, Clone)]
pub struct ExecLoadPlan {
    pub entry: u64,
    pub image_len: usize,
    pub segments: Vec<ExecLoadSegmentPlan>,
    pub segment_pages: u64,
    pub stack_top: u64,
    pub stack_pages: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExecMapPreview {
    pub segment_count: usize,
    pub executable_segments: usize,
    pub writable_segments: usize,
    pub segment_pages: u64,
    pub stack_pages: u64,
    pub total_pages: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMappedPageKind {
    Segment,
    Stack,
}

#[derive(Debug, Clone, Copy)]
pub struct ExecMappedPage {
    pub virt: u64,
    pub phys: u64,
    pub flags: u64,
    pub kind: ExecMappedPageKind,
}

#[derive(Clone)]
struct ExecVmaPlan {
    start: u64,
    end: u64,
    info: mm::VmaInfo,
}

#[derive(Clone, Copy)]
struct ExecMmLayoutSnapshot {
    start_code: u64,
    end_code: u64,
    start_data: u64,
    end_data: u64,
    start_brk: u64,
    brk: u64,
    start_stack: u64,
    arg_start: u64,
    arg_end: u64,
    env_start: u64,
    env_end: u64,
    mmap_base: u64,
    mmap_legacy_base: u64,
}

#[inline]
unsafe fn write_user_bytes(dst: u64, src: &[u8]) -> Result<(), i32> {
    if src.is_empty() {
        return Ok(());
    }

    let end = dst.checked_add(src.len() as u64).ok_or(E2BIG)?;
    if !mm::is_user_addr(dst) || !mm::is_user_addr(end - 1) {
        return Err(EFAULT);
    }

    core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut u8, src.len());
    Ok(())
}

#[inline]
unsafe fn write_user_usize(dst: u64, value: usize) -> Result<(), i32> {
    let end = dst
        .checked_add(core::mem::size_of::<usize>() as u64)
        .ok_or(E2BIG)?;
    if !mm::is_user_addr(dst) || !mm::is_user_addr(end - 1) {
        return Err(EFAULT);
    }

    core::ptr::write(dst as *mut usize, value);
    Ok(())
}

pub fn setup_initial_user_stack(
    stack_top: u64,
    stack_pages: u64,
    argv: &[&str],
    envp: &[&str],
) -> Result<u64, i32> {
    if stack_pages == 0 {
        return Err(EINVAL);
    }

    let stack_span = stack_pages.checked_mul(mm::PAGE_SIZE).ok_or(E2BIG)?;
    let stack_bottom = stack_top.checked_sub(stack_span).ok_or(E2BIG)?;
    let mut sp = stack_top;
    let mut argv_ptrs: Vec<u64> = Vec::with_capacity(argv.len());
    let mut envp_ptrs: Vec<u64> = Vec::with_capacity(envp.len());

    for value in envp.iter().rev() {
        let bytes = value.as_bytes();
        let reserve = (bytes.len() as u64).saturating_add(1);
        sp = sp.checked_sub(reserve).ok_or(E2BIG)?;
        if sp < stack_bottom {
            return Err(E2BIG);
        }
        unsafe {
            write_user_bytes(sp, bytes)?;
            write_user_bytes(sp + bytes.len() as u64, &[0])?;
        }
        envp_ptrs.push(sp);
    }
    envp_ptrs.reverse();

    for value in argv.iter().rev() {
        let bytes = value.as_bytes();
        let reserve = (bytes.len() as u64).saturating_add(1);
        sp = sp.checked_sub(reserve).ok_or(E2BIG)?;
        if sp < stack_bottom {
            return Err(E2BIG);
        }
        unsafe {
            write_user_bytes(sp, bytes)?;
            write_user_bytes(sp + bytes.len() as u64, &[0])?;
        }
        argv_ptrs.push(sp);
    }
    argv_ptrs.reverse();

    sp &= !0xFu64;
    let word = core::mem::size_of::<usize>() as u64;
    let planned_pushes = argv_ptrs
        .len()
        .saturating_add(envp_ptrs.len())
        .saturating_add(3);
    if planned_pushes % 2 == 0 {
        sp = sp.checked_sub(word).ok_or(E2BIG)?;
        if sp < stack_bottom {
            return Err(E2BIG);
        }
    }
    let mut push = |value: usize| -> Result<(), i32> {
        sp = sp.checked_sub(word).ok_or(E2BIG)?;
        if sp < stack_bottom {
            return Err(E2BIG);
        }
        unsafe { write_user_usize(sp, value) }
    };

    push(0)?;
    for ptr in envp_ptrs.iter().rev() {
        push(*ptr as usize)?;
    }
    let env_start = envp_ptrs.first().copied().unwrap_or(0);
    let env_end = envp_ptrs
        .iter()
        .zip(envp.iter())
        .last()
        .map(|(ptr, text)| ptr.saturating_add(text.len() as u64 + 1))
        .unwrap_or(0);

    push(0)?;
    for ptr in argv_ptrs.iter().rev() {
        push(*ptr as usize)?;
    }
    let arg_start = argv_ptrs.first().copied().unwrap_or(0);
    let arg_end = argv_ptrs
        .iter()
        .zip(argv.iter())
        .last()
        .map(|(ptr, text)| ptr.saturating_add(text.len() as u64 + 1))
        .unwrap_or(0);

    push(argv.len())?;

    let mm_ptr = crate::task::current_mm_ptr();
    if !mm_ptr.is_null() {
        unsafe {
            (*mm_ptr).arg_start = arg_start;
            (*mm_ptr).arg_end = arg_end;
            (*mm_ptr).env_start = env_start;
            (*mm_ptr).env_end = env_end;
        }
    }

    Ok(sp)
}

#[inline]
fn read_struct_unaligned<T: Copy>(buf: &[u8], offset: usize) -> Option<T> {
    let size = size_of::<T>();
    let end = offset.checked_add(size)?;
    if end > buf.len() {
        return None;
    }

    Some(unsafe { core::ptr::read_unaligned(buf.as_ptr().add(offset) as *const T) })
}

#[inline]
fn is_power_of_two(value: u64) -> bool {
    value != 0 && (value & (value - 1)) == 0
}

#[inline]
fn segment_contains_entry(segment: &ExecLoadSegmentPlan, entry: u64) -> bool {
    match segment.vaddr.checked_add(segment.mem_size) {
        Some(end) => entry >= segment.vaddr && entry < end,
        None => false,
    }
}

#[inline]
fn ranges_overlap(start_a: u64, end_a: u64, start_b: u64, end_b: u64) -> bool {
    start_a < end_b && start_b < end_a
}

#[inline]
fn read_cr3_phys() -> u64 {
    mm::arch::read_cr3() & mm::PTE_ADDR_MASK
}

#[inline]
fn current_mm_pgd() -> u64 {
    let mm_ptr = crate::task::current_mm_ptr();
    if mm_ptr.is_null() {
        read_cr3_phys()
    } else {
        unsafe { (*mm_ptr).pgd }
    }
}

#[inline]
fn current_page_table_manager() -> mm::PageTableManager {
    let pml4_phys = current_mm_pgd();
    unsafe { mm::PageTableManager::new(pml4_phys, mm::direct_map_offset()) }
}

fn build_exec_vma_plan(plan: &ExecLoadPlan) -> Vec<ExecVmaPlan> {
    let mut vmas: Vec<ExecVmaPlan> = Vec::with_capacity(plan.segments.len().saturating_add(1));

    for segment in plan.segments.iter() {
        let mut flags = mm::VmFlags::empty();
        flags.set(mm::VmFlags::READ);
        flags.set(mm::VmFlags::ANONYMOUS);
        if segment.executable {
            flags.set(mm::VmFlags::EXEC);
            flags.set(mm::VmFlags::CODE);
        }
        if segment.writable {
            flags.set(mm::VmFlags::WRITE);
            flags.set(mm::VmFlags::MAYWRITE);
            flags.set(mm::VmFlags::DATA);
        }

        vmas.push(ExecVmaPlan {
            start: segment.page_start,
            end: segment.page_end,
            info: mm::VmaInfo::new(flags),
        });
    }

    let stack_bytes = plan.stack_pages.saturating_mul(mm::PAGE_SIZE);
    let stack_bottom = plan.stack_top.saturating_sub(stack_bytes);
    let mut stack_flags = mm::VmFlags::empty();
    stack_flags.set(mm::VmFlags::READ);
    stack_flags.set(mm::VmFlags::WRITE);
    stack_flags.set(mm::VmFlags::MAYWRITE);
    stack_flags.set(mm::VmFlags::ANONYMOUS);
    stack_flags.set(mm::VmFlags::GROWSDOWN);
    vmas.push(ExecVmaPlan {
        start: stack_bottom,
        end: plan.stack_top,
        info: mm::VmaInfo::new(stack_flags),
    });

    vmas
}

fn snapshot_mm_layout(mm_state: &mm::Mm) -> ExecMmLayoutSnapshot {
    ExecMmLayoutSnapshot {
        start_code: mm_state.start_code,
        end_code: mm_state.end_code,
        start_data: mm_state.start_data,
        end_data: mm_state.end_data,
        start_brk: mm_state.start_brk,
        brk: mm_state.brk,
        start_stack: mm_state.start_stack,
        arg_start: mm_state.arg_start,
        arg_end: mm_state.arg_end,
        env_start: mm_state.env_start,
        env_end: mm_state.env_end,
        mmap_base: mm_state.mmap_base,
        mmap_legacy_base: mm_state.mmap_legacy_base,
    }
}

fn restore_mm_layout(mm_state: &mut mm::Mm, snapshot: ExecMmLayoutSnapshot) {
    mm_state.start_code = snapshot.start_code;
    mm_state.end_code = snapshot.end_code;
    mm_state.start_data = snapshot.start_data;
    mm_state.end_data = snapshot.end_data;
    mm_state.start_brk = snapshot.start_brk;
    mm_state.brk = snapshot.brk;
    mm_state.start_stack = snapshot.start_stack;
    mm_state.arg_start = snapshot.arg_start;
    mm_state.arg_end = snapshot.arg_end;
    mm_state.env_start = snapshot.env_start;
    mm_state.env_end = snapshot.env_end;
    mm_state.mmap_base = snapshot.mmap_base;
    mm_state.mmap_legacy_base = snapshot.mmap_legacy_base;
}

pub fn install_current_exec_vmas(plan: &ExecLoadPlan) -> Result<(), i32> {
    let mm_ptr = crate::task::current_mm_ptr();
    if mm_ptr.is_null() {
        return Err(ESRCH);
    }

    let new_vmas = build_exec_vma_plan(plan);
    let mm_state = unsafe { &mut *mm_ptr };
    let old_layout = snapshot_mm_layout(mm_state);
    let old_user_vmas = {
        let _guard = mm_state.lock.lock();
        mm_state
            .vma_tree
            .iter()
            .filter_map(|(start, end, info)| {
                let start = start as u64;
                let end = end as u64;
                (end > mm::USER_SPACE_START && start < mm::USER_SPACE_END)
                    .then_some(ExecVmaPlan {
                        start,
                        end,
                        info: info.clone(),
                    })
            })
            .collect::<Vec<_>>()
    };

    for vma in old_user_vmas.iter() {
        let _ = mm_state.remove_vma(vma.start);
    }

    let mut inserted: Vec<ExecVmaPlan> = Vec::new();
    for vma in new_vmas.iter() {
        if !mm_state.insert_vma(vma.start, vma.end, vma.info.clone()) {
            for inserted_vma in inserted.iter().rev() {
                let _ = mm_state.remove_vma(inserted_vma.start);
            }
            for old_vma in old_user_vmas.iter() {
                let _ = mm_state.insert_vma(old_vma.start, old_vma.end, old_vma.info.clone());
            }
            restore_mm_layout(mm_state, old_layout);
            return Err(EBUSY);
        }
        inserted.push(vma.clone());
    }

    let mut start_code = 0u64;
    let mut end_code = 0u64;
    let mut start_data = 0u64;
    let mut end_data = 0u64;
    let mut brk_base = 0u64;
    for segment in plan.segments.iter() {
        if segment.executable {
            if start_code == 0 || segment.page_start < start_code {
                start_code = segment.page_start;
            }
            end_code = end_code.max(segment.page_end);
        }
        if segment.writable {
            if start_data == 0 || segment.page_start < start_data {
                start_data = segment.page_start;
            }
            end_data = end_data.max(segment.page_end);
            brk_base = brk_base.max(segment.page_end);
        }
    }

    mm_state.start_code = start_code;
    mm_state.end_code = end_code;
    mm_state.start_data = start_data;
    mm_state.end_data = end_data;
    mm_state.start_brk = brk_base;
    mm_state.brk = brk_base;
    mm_state.start_stack = plan.stack_top;
    mm_state.arg_start = 0;
    mm_state.arg_end = 0;
    mm_state.env_start = 0;
    mm_state.env_end = 0;
    mm_state.mmap_base = mm::USER_MMAP_BASE;
    mm_state.mmap_legacy_base = mm::USER_MMAP_BASE;

    Ok(())
}

#[inline]
fn user_zero_gfp() -> mm::GfpFlags {
    let mut gfp = mm::GFP_USER;
    gfp.set(mm::GfpFlags::ZERO);
    gfp
}

fn log_mapping_conflict(pt_mgr: &mm::PageTableManager, virt: u64, tag: &str) {
    if let Some((entry, level, page_size)) = pt_mgr.translate(virt) {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] map conflict tag={} virt={:#x} phys={:#x} level={:?} page_size={:#x} flags={:#x}",
                tag,
                virt,
                entry.phys_addr(),
                level,
                page_size,
                entry.flags(),
            );
        }
        return;
    }

    if let Some(phys) = pt_mgr.translate_addr(virt) {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] map conflict tag={} virt={:#x} phys={:#x} (translate_addr fallback)",
                tag,
                virt,
                phys,
            );
        }
        return;
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] map conflict tag={} virt={:#x} (translation unavailable)",
            tag,
            virt,
        );
    }
}

fn validate_target_unmapped(
    pt_mgr: &mm::PageTableManager,
    plan: &ExecLoadPlan,
    stack_bottom: u64,
) -> Result<(), i32> {
    for (segment_idx, segment) in plan.segments.iter().enumerate() {
        for page_idx in 0..segment.page_count {
            let page_offset = page_idx.checked_mul(mm::PAGE_SIZE).ok_or(E2BIG)?;
            let virt = segment.page_start.checked_add(page_offset).ok_or(E2BIG)?;
            if pt_mgr.translate_addr(virt).is_some() {
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[execve] segment page already mapped seg={} page_idx={} range=[{:#x}, {:#x})",
                        segment_idx,
                        page_idx,
                        segment.page_start,
                        segment.page_end,
                    );
                }
                log_mapping_conflict(pt_mgr, virt, "segment");
                return Err(EBUSY);
            }
        }
    }

    for page_idx in 0..plan.stack_pages {
        let page_offset = page_idx.checked_mul(mm::PAGE_SIZE).ok_or(E2BIG)?;
        let virt = stack_bottom.checked_add(page_offset).ok_or(E2BIG)?;
        if pt_mgr.translate_addr(virt).is_some() {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] stack page already mapped page_idx={} range=[{:#x}, {:#x})",
                    page_idx,
                    stack_bottom,
                    plan.stack_top,
                );
            }
            log_mapping_conflict(pt_mgr, virt, "stack");
            return Err(EBUSY);
        }
    }

    Ok(())
}

fn map_zero_page(
    pt_mgr: &mm::PageTableManager,
    virt: u64,
    flags: u64,
    kind: ExecMappedPageKind,
    mapped_pages: &mut Vec<ExecMappedPage>,
) -> Result<u64, i32> {
    let page = mm::alloc_page(user_zero_gfp()).ok_or(ENOMEM)?;
    let page_ptr = page as *mut mm::Page as usize;
    let pfn = mm::page_to_pfn(page);
    let max_pfn = mm::max_pfn();

    if pfn >= max_pfn {
        let vmemmap_base = mm::vmemmap_base_ptr();
        let raw_offset = if vmemmap_base.is_null() {
            0isize
        } else {
            unsafe { (page as *const mm::Page).offset_from(vmemmap_base as *const mm::Page) }
        };
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] map_zero_page invalid page metadata kind={:?} virt={:#x} page_ptr={:#x} pfn={} max_pfn={} vmemmap_base={:#x} raw_offset={}",
                kind,
                virt,
                page_ptr,
                pfn,
                max_pfn,
                vmemmap_base as usize,
                raw_offset,
            );
        }
        return Err(E2BIG);
    }

    let phys = match pfn.checked_mul(mm::PAGE_SIZE) {
        Some(v) => v,
        None => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] map_zero_page pfn overflow kind={:?} virt={:#x} page_ptr={:#x} pfn={} page_size={:#x}",
                    kind,
                    virt,
                    page_ptr,
                    pfn,
                    mm::PAGE_SIZE,
                );
            }
            return Err(E2BIG);
        }
    };

    let mapped_ok = unsafe { pt_mgr.map_page(virt, phys, flags) };
    if !mapped_ok {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] map_zero_page map_page failed kind={:?} virt={:#x} phys={:#x}",
                kind,
                virt,
                phys,
            );
        }
        unsafe {
            mm::free_page(page);
        }
        return Err(ENOMEM);
    }

    page.inc_mapcount();

    mapped_pages.push(ExecMappedPage {
        virt,
        phys,
        flags,
        kind,
    });

    Ok(phys)
}

fn copy_segment_page_data(
    image: &[u8],
    segment: &ExecLoadSegmentPlan,
    virt_page_start: u64,
    phys_page_start: u64,
) -> Result<(), i32> {
    let segment_file_end = segment.vaddr.checked_add(segment.file_size).ok_or(E2BIG)?;

    if segment.file_size == 0 {
        return Ok(());
    }

    let page_end = virt_page_start.checked_add(mm::PAGE_SIZE).ok_or(E2BIG)?;
    let copy_start = cmp::max(virt_page_start, segment.vaddr);
    let copy_end = cmp::min(page_end, segment_file_end);

    if copy_end <= copy_start {
        return Ok(());
    }

    let src_delta = copy_start.checked_sub(segment.vaddr).ok_or(EINVAL)?;
    let src_offset_u64 = segment.file_offset.checked_add(src_delta).ok_or(E2BIG)?;
    let src_offset = usize::try_from(src_offset_u64).map_err(|_| E2BIG)?;
    let copy_len = usize::try_from(copy_end - copy_start).map_err(|_| E2BIG)?;
    let src_end = src_offset.checked_add(copy_len).ok_or(E2BIG)?;

    if src_end > image.len() {
        return Err(EINVAL);
    }

    let page_offset = copy_start.checked_sub(virt_page_start).ok_or(EINVAL)?;
    let dst_virt = mm::phys_to_virt(phys_page_start)
        .checked_add(page_offset)
        .ok_or(E2BIG)?;

    unsafe {
        core::ptr::copy_nonoverlapping(
            image.as_ptr().add(src_offset),
            dst_virt as *mut u8,
            copy_len,
        );
    }

    Ok(())
}

pub fn build_elf_load_plan(image: &[u8]) -> Result<ExecLoadPlan, i32> {
    let header = read_struct_unaligned::<Elf64Header>(image, 0).ok_or(EINVAL)?;

    if header.ident[0..4] != ELF_MAGIC {
        return Err(EINVAL);
    }
    if header.ident[4] != ELF_CLASS_64 {
        return Err(EINVAL);
    }
    if header.ident[5] != ELF_DATA_LSB {
        return Err(EINVAL);
    }
    if header.ident[6] != ELF_VERSION_CURRENT {
        return Err(EINVAL);
    }
    if header.e_version != ELF_VERSION_CURRENT as u32 {
        return Err(EINVAL);
    }
    if header.e_machine != crate::task::arch::supported_elf_machine() {
        return Err(EINVAL);
    }
    if header.e_phentsize as usize != size_of::<Elf64ProgramHeader>() {
        return Err(EINVAL);
    }
    if header.e_phnum == 0 {
        return Err(ENOENT);
    }

    let phoff = usize::try_from(header.e_phoff).map_err(|_| EINVAL)?;
    let phentsize = header.e_phentsize as usize;
    let phnum = header.e_phnum as usize;

    let ph_table_size = phentsize.checked_mul(phnum).ok_or(E2BIG)?;
    let ph_table_end = phoff.checked_add(ph_table_size).ok_or(E2BIG)?;
    if ph_table_end > image.len() {
        return Err(EINVAL);
    }

    let mut segments: Vec<ExecLoadSegmentPlan> = Vec::new();
    let mut segment_pages = 0u64;

    for index in 0..phnum {
        let ph_offset = phoff
            .checked_add(index.checked_mul(phentsize).ok_or(E2BIG)?)
            .ok_or(E2BIG)?;
        let ph = read_struct_unaligned::<Elf64ProgramHeader>(image, ph_offset).ok_or(EINVAL)?;

        if ph.p_type != PT_LOAD {
            continue;
        }

        if ph.p_memsz == 0 {
            continue;
        }

        if ph.p_filesz > ph.p_memsz {
            return Err(EINVAL);
        }

        let file_end = ph.p_offset.checked_add(ph.p_filesz).ok_or(E2BIG)?;
        if file_end > image.len() as u64 {
            return Err(EINVAL);
        }

        let seg_start = ph.p_vaddr;
        let seg_end = ph.p_vaddr.checked_add(ph.p_memsz).ok_or(E2BIG)?;

        if seg_start < mm::USER_SPACE_START {
            return Err(EINVAL);
        }
        if seg_end <= seg_start {
            return Err(EINVAL);
        }
        if !mm::is_user_addr(seg_start) || !mm::is_user_addr(seg_end - 1) {
            return Err(EINVAL);
        }

        if ph.p_align != 0 && !is_power_of_two(ph.p_align) {
            return Err(EINVAL);
        }

        let page_start = mm::page_align_down(seg_start);
        let page_end = mm::page_align_up(seg_end);
        let page_count = (page_end - page_start) / mm::PAGE_SIZE;

        let mut map_flags = mm::PTE_PRESENT | mm::PTE_USER;
        let writable = (ph.p_flags & PF_W) != 0;
        let executable = (ph.p_flags & PF_X) != 0;

        if writable {
            map_flags |= mm::PTE_WRITABLE;
        }
        if !executable {
            map_flags |= mm::PTE_NO_EXECUTE;
        }

        segment_pages = segment_pages.checked_add(page_count).ok_or(E2BIG)?;

        segments.push(ExecLoadSegmentPlan {
            vaddr: seg_start,
            mem_size: ph.p_memsz,
            file_offset: ph.p_offset,
            file_size: ph.p_filesz,
            align: ph.p_align,
            page_start,
            page_end,
            page_count,
            map_flags,
            executable,
            writable,
        });
    }

    if segments.is_empty() {
        return Err(ENOENT);
    }

    segments.sort_by_key(|segment| segment.page_start);
    for idx in 1..segments.len() {
        let previous = &segments[idx - 1];
        let current = &segments[idx];
        if previous.page_end > current.page_start {
            return Err(EINVAL);
        }
    }

    let entry = header.e_entry;
    if entry < mm::USER_SPACE_START || !mm::is_user_addr(entry) {
        return Err(EINVAL);
    }

    if !segments
        .iter()
        .any(|segment| segment_contains_entry(segment, entry))
    {
        return Err(EINVAL);
    }

    Ok(ExecLoadPlan {
        entry,
        image_len: image.len(),
        segments,
        segment_pages,
        stack_top: mm::USER_STACK_TOP,
        stack_pages: DEFAULT_USER_STACK_PAGES,
    })
}

pub fn preview_pt_load_mapping(plan: &ExecLoadPlan) -> ExecMapPreview {
    let mut preview = ExecMapPreview {
        segment_count: plan.segments.len(),
        segment_pages: plan.segment_pages,
        stack_pages: plan.stack_pages,
        total_pages: plan.segment_pages.saturating_add(plan.stack_pages),
        ..ExecMapPreview::default()
    };

    for segment in plan.segments.iter() {
        if segment.executable {
            preview.executable_segments = preview.executable_segments.saturating_add(1);
        }
        if segment.writable {
            preview.writable_segments = preview.writable_segments.saturating_add(1);
        }
    }

    preview
}

pub fn stage_pt_load_mappings(
    image: &[u8],
    plan: &ExecLoadPlan,
) -> Result<Vec<ExecMappedPage>, i32> {
    let mut mapped_pages: Vec<ExecMappedPage> = Vec::new();

    let stage_result = (|| {
        let stack_bytes = plan.stack_pages.checked_mul(mm::PAGE_SIZE).ok_or(E2BIG)?;
        let stack_bottom = plan.stack_top.checked_sub(stack_bytes).ok_or(EINVAL)?;

        if stack_bottom < mm::USER_SPACE_START {
            return Err(EINVAL);
        }
        if plan.stack_top <= stack_bottom {
            return Err(EINVAL);
        }
        if !mm::is_user_addr(stack_bottom) || !mm::is_user_addr(plan.stack_top - 1) {
            return Err(EINVAL);
        }

        for segment in plan.segments.iter() {
            if ranges_overlap(
                segment.page_start,
                segment.page_end,
                stack_bottom,
                plan.stack_top,
            ) {
                return Err(EINVAL);
            }
        }

        let pt_mgr = current_page_table_manager();
        validate_target_unmapped(&pt_mgr, plan, stack_bottom)?;

        for segment in plan.segments.iter() {
            for page_idx in 0..segment.page_count {
                let page_offset = page_idx.checked_mul(mm::PAGE_SIZE).ok_or(E2BIG)?;
                let virt = segment.page_start.checked_add(page_offset).ok_or(E2BIG)?;

                let phys = map_zero_page(
                    &pt_mgr,
                    virt,
                    segment.map_flags,
                    ExecMappedPageKind::Segment,
                    &mut mapped_pages,
                )?;

                copy_segment_page_data(image, segment, virt, phys)?;
            }
        }

        let stack_flags = mm::PTE_PRESENT | mm::PTE_USER | mm::PTE_WRITABLE | mm::PTE_NO_EXECUTE;
        for page_idx in 0..plan.stack_pages {
            let page_offset = page_idx.checked_mul(mm::PAGE_SIZE).ok_or(E2BIG)?;
            let virt = stack_bottom.checked_add(page_offset).ok_or(E2BIG)?;

            map_zero_page(
                &pt_mgr,
                virt,
                stack_flags,
                ExecMappedPageKind::Stack,
                &mut mapped_pages,
            )?;
        }

        Ok(())
    })();

    if let Err(errno) = stage_result {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] stage map rollback errno={} mapped_pages={}",
                errno,
                mapped_pages.len()
            );
        }
        rollback_exec_mappings(&mapped_pages);
        return Err(errno);
    }

    Ok(mapped_pages)
}

fn release_mapped_phys_page(phys: u64) {
    let pfn = phys / mm::PAGE_SIZE;

    unsafe {
        if pfn >= mm::max_pfn() {
            return;
        }

        let page = &mut *mm::pfn_to_page(pfn);

        if page.mapcount() >= 0 {
            let _ = page.try_dec_mapcount();
        }

        if page.refcount() == 0 {
            return;
        }

        mm::free_page(page);
    }
}

pub fn rollback_exec_mappings(mapped_pages: &[ExecMappedPage]) {
    if mapped_pages.is_empty() {
        return;
    }

    let pt_mgr = current_page_table_manager();

    for page in mapped_pages.iter().rev() {
        unsafe {
            let _ = pt_mgr.unmap_page(page.virt);
        }
        release_mapped_phys_page(page.phys);
    }
}
