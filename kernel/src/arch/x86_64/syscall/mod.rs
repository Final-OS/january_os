use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::interrupt::arch::x86_64::entry::gdt::{KERNEL_CODE_SELECTOR, USER_CODE_SELECTOR, USER_DATA_SELECTOR};

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;
const EFER_SCE: u64 = 1 << 0;
const EFER_NXE: u64 = 1 << 11;

const FMASK_CLEAR_FLAGS: u64 = (1 << 9) | (1 << 8);
const SYSCALL_TRACE_VERBOSE: bool = false;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct SavedSyscallFrame {
    user_rsp: u64,
    user_rflags: u64,
    user_rip: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbp: u64,
    rbx: u64,
    r9: u64,
    r8: u64,
    r10: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rax: u64,
}

struct CapturedSyscallFrame {
    valid: AtomicBool,
    frame: UnsafeCell<SavedSyscallFrame>,
}

unsafe impl Sync for CapturedSyscallFrame {}

impl CapturedSyscallFrame {
    const fn new() -> Self {
        Self {
            valid: AtomicBool::new(false),
            frame: UnsafeCell::new(SavedSyscallFrame {
                user_rsp: 0,
                user_rflags: 0,
                user_rip: 0,
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                rbp: 0,
                rbx: 0,
                r9: 0,
                r8: 0,
                r10: 0,
                rdx: 0,
                rsi: 0,
                rdi: 0,
                rax: 0,
            }),
        }
    }
}

/// 每 CPU 的 syscall 上下文槽位数量（按 APIC ID 索引）。
const SYSCALL_RSP_SLOT_COUNT: usize = if crate::config::MAX_APIC_IDS > 0 {
    crate::config::MAX_APIC_IDS
} else {
    1
};

#[repr(C)]
struct SyscallCpuContext {
    /// 当前 CPU 进入用户态前武装的内核栈顶
    kernel_rsp: AtomicU64,
    /// 入口切栈前暂存用户态 RSP（仅汇编入口使用）
    user_rsp_scratch: AtomicU64,
}

impl SyscallCpuContext {
    const fn new() -> Self {
        Self {
            kernel_rsp: AtomicU64::new(0),
            user_rsp_scratch: AtomicU64::new(0),
        }
    }
}

#[unsafe(no_mangle)]
static SYSCALL_CPU_CONTEXTS: [SyscallCpuContext; SYSCALL_RSP_SLOT_COUNT] =
    [const { SyscallCpuContext::new() }; SYSCALL_RSP_SLOT_COUNT];

static SYSCALL_DIAG_SEQ: AtomicU64 = AtomicU64::new(0);
static SYSCALL_INIT_LOGGED: AtomicBool = AtomicBool::new(false);
static CAPTURED_SYSCALL_FRAMES: [CapturedSyscallFrame; SYSCALL_RSP_SLOT_COUNT] =
    [const { CapturedSyscallFrame::new() }; SYSCALL_RSP_SLOT_COUNT];

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;

    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, preserves_flags)
    );

    ((high as u64) << 32) | (low as u64)
}

#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;

    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, preserves_flags)
    );
}

#[inline]
fn read_syscall_kernel_rsp() -> u64 {
    let idx = syscall_slot_index_current_cpu();
    SYSCALL_CPU_CONTEXTS[idx].kernel_rsp.load(Ordering::Acquire)
}

#[inline]
pub fn set_syscall_kernel_rsp(rsp: u64) {
    let idx = syscall_slot_index_current_cpu();
    SYSCALL_CPU_CONTEXTS[idx]
        .kernel_rsp
        .store(rsp, Ordering::Release);
    SYSCALL_CPU_CONTEXTS[idx]
        .user_rsp_scratch
        .store(0, Ordering::Release);

    // syscall 入口通过 swapgs + gs:[offset] 直接读取当前 CPU 上下文，
    // 避免在用户栈上执行任何 push/call。
    let ctx_ptr = core::ptr::addr_of!(SYSCALL_CPU_CONTEXTS[idx]) as u64;
    unsafe {
        wrmsr(IA32_KERNEL_GS_BASE, ctx_ptr);
    }
}

#[inline]
fn syscall_slot_index_current_cpu() -> usize {
    if !crate::interrupt::apic_initialized() {
        return 0;
    }
    let apic_id = crate::interrupt::local_apic_id() as usize;
    if apic_id < SYSCALL_RSP_SLOT_COUNT {
        apic_id
    } else {
        0
    }
}

#[inline]
fn current_saved_syscall_frame() -> Option<SavedSyscallFrame> {
    let idx = syscall_slot_index_current_cpu();
    let slot = &CAPTURED_SYSCALL_FRAMES[idx];
    if !slot.valid.load(Ordering::Acquire) {
        return None;
    }

    Some(unsafe { *slot.frame.get() })
}

#[inline]
pub fn current_fork_return_frame() -> Option<crate::task::arch::ForkReturnFrame> {
    let frame = current_saved_syscall_frame()?;
    Some(crate::task::arch::ForkReturnFrame {
        rip: frame.user_rip,
        rsp: frame.user_rsp,
        rflags: frame.user_rflags,
        r15: frame.r15,
        r14: frame.r14,
        r13: frame.r13,
        r12: frame.r12,
        rbp: frame.rbp,
        rbx: frame.rbx,
        r9: frame.r9,
        r8: frame.r8,
        r10: frame.r10,
        rdx: frame.rdx,
        rsi: frame.rsi,
        rdi: frame.rdi,
        rax: 0,
    })
}

#[unsafe(no_mangle)]
extern "C" fn syscall_dispatch_from_asm(frame: *const SavedSyscallFrame) -> usize {
    let Some(frame) = (unsafe { frame.as_ref() }) else {
        return (-(crate::errno::EINVAL as isize)) as usize;
    };

    let idx = syscall_slot_index_current_cpu();
    let slot = &CAPTURED_SYSCALL_FRAMES[idx];
    unsafe {
        *slot.frame.get() = *frame;
    }
    slot.valid.store(true, Ordering::Release);

    let seq = SYSCALL_DIAG_SEQ.fetch_add(1, Ordering::Relaxed);
    let should_log = seq < 32 || frame.rax == 59 || frame.rax == 60 || frame.rax == 231;

    if should_log {
        if crate::config::DEBUG_VERBOSE && SYSCALL_TRACE_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[syscall] enter seq={} nr={} args=[{:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x}] k_rsp={:#x}",
                seq,
                frame.rax,
                frame.rdi,
                frame.rsi,
                frame.rdx,
                frame.r10,
                frame.r8,
                frame.r9,
                read_syscall_kernel_rsp(),
            );
        }
    }

    let ret = crate::syscall::dispatch(
        frame.rax as usize,
        frame.rdi as usize,
        frame.rsi as usize,
        frame.rdx as usize,
        frame.r10 as usize,
        frame.r8 as usize,
        frame.r9 as usize,
    );

    slot.valid.store(false, Ordering::Release);

    if should_log {
        if crate::config::DEBUG_VERBOSE && SYSCALL_TRACE_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[syscall] leave seq={} nr={} ret={:#x}",
                seq,
                frame.rax,
                ret,
            );
        }
    }

    ret
}

unsafe extern "C" {
    fn syscall_entry();
}

pub unsafe fn init_syscall() {
    crate::smp::ap_boot_probe_set_stage(330);
    let kernel_cs = (KERNEL_CODE_SELECTOR & !0x3) as u64;

    // Intel/AMD 约定：64 位 SYSRET 的 CS 来自 STAR[63:48] + 16。
    // 当前返回路径使用 iretq，但仍写入兼容值。
    let user_sysret_base = ((USER_CODE_SELECTOR & !0x3) as u64).saturating_sub(16);

    let star = (kernel_cs << 32) | (user_sysret_base << 48);
    crate::smp::ap_boot_probe_set_stage(331);
    wrmsr(IA32_STAR, star);
    crate::smp::ap_boot_probe_set_stage(332);
    wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    crate::smp::ap_boot_probe_set_stage(333);
    wrmsr(IA32_FMASK, FMASK_CLEAR_FLAGS);
    crate::smp::ap_boot_probe_set_stage(334);

    let mut efer = rdmsr(IA32_EFER);
    crate::smp::ap_boot_probe_set_stage(335);
    efer |= EFER_SCE | EFER_NXE;
    wrmsr(IA32_EFER, efer);
    crate::smp::ap_boot_probe_set_stage(336);

    if SYSCALL_INIT_LOGGED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_ok()
    {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[syscall] init STAR={:#x} LSTAR={:#x} FMASK={:#x}",
                star,
                syscall_entry as *const () as usize,
                FMASK_CLEAR_FLAGS,
            );
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub nr: usize,
    pub arg0: usize,
    pub arg1: usize,
    pub arg2: usize,
    pub arg3: usize,
    pub arg4: usize,
    pub arg5: usize,
}

pub fn handle(frame: &SyscallFrame) -> usize {
    crate::syscall::dispatch(
        frame.nr, frame.arg0, frame.arg1, frame.arg2, frame.arg3, frame.arg4, frame.arg5,
    )
}

global_asm!(
    r#"
    .section .text
    .global syscall_entry
    .type syscall_entry, @function
syscall_entry:
    cld

    // 立即切换到当前 CPU 的内核 GS 基址，并在不触碰用户栈的前提下切栈。
    // gs:[0] = kernel_rsp, gs:[8] = user_rsp_scratch
    swapgs
    mov qword ptr gs:[8], rsp
    mov rsp, qword ptr gs:[0]
    test rsp, rsp
    jnz 1f
    ud2
1:
    // 在内核栈保存用户态关键信息：
    // [user_rsp, r11(flags), rcx(rip), r15, r14, r13, r12, rbp, rbx, r9, r8, r10, rdx, rsi, rdi, rax(nr)]
    push rax
    push rdi
    push rsi
    push rdx
    push r10
    push r8
    push r9
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    push rcx
    push r11
    push qword ptr gs:[8]

    // r13: 用户保存帧基址（位于当前内核栈）
    mov r13, rsp

    // SysV 调用对齐：call 前 rsp 16-byte 对齐
    and rsp, -16

    mov rdi, r13
    call syscall_dispatch_from_asm

    // 恢复用户寄存器（rax 保留 syscall 返回值）
    mov r15, [r13 + 24]
    mov r14, [r13 + 32]
    mov r12, [r13 + 48]
    mov rbp, [r13 + 56]
    mov rbx, [r13 + 64]
    mov r9,  [r13 + 72]
    mov r8,  [r13 + 80]
    mov r10, [r13 + 88]
    mov rdx, [r13 + 96]
    mov rsi, [r13 + 104]
    mov rdi, [r13 + 112]

    // iretq 返回帧：SS, RSP, RFLAGS, CS, RIP
    mov rcx, [r13 + 16]
    mov r11, [r13 + 8]
    mov rdx, [r13 + 0]

    push {user_data_sel}
    push rdx
    push r11
    push {user_code_sel}
    push rcx

    // r13 最后恢复
    mov r13, [r13 + 40]
    // 恢复用户 GS 基址并保留内核 GS 基址到 IA32_KERNEL_GS_BASE。
    swapgs

    iretq
"#,
    user_code_sel = const USER_CODE_SELECTOR as u64,
    user_data_sel = const USER_DATA_SELECTOR as u64,
);
