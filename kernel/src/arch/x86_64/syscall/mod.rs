use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::interrupt::{KERNEL_CODE_SELECTOR, USER_CODE_SELECTOR, USER_DATA_SELECTOR};

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;

const EFER_SCE: u64 = 1 << 0;

const FMASK_CLEAR_FLAGS: u64 = (1 << 9) | (1 << 8);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RawSyscallFrame {
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
}

/// 进入用户态前写入：`syscall` 指令进入 ring0 后使用的内核栈指针。
///
/// 说明：当前实现先服务于 runuser/ring3 演示链路，后续会收敛到 per-cpu/per-task
/// 的正式上下文管理。
#[unsafe(no_mangle)]
static mut SYSCALL_KERNEL_RSP_SLOT: u64 = 0;

static SYSCALL_DIAG_SEQ: AtomicU64 = AtomicU64::new(0);

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
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(SYSCALL_KERNEL_RSP_SLOT)) }
}

#[inline]
pub fn set_syscall_kernel_rsp(rsp: u64) {
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(SYSCALL_KERNEL_RSP_SLOT), rsp);
    }
}

#[unsafe(no_mangle)]
extern "C" fn syscall_dispatch_from_asm(frame: *const RawSyscallFrame) -> usize {
    let Some(frame) = (unsafe { frame.as_ref() }) else {
        return (-(crate::syscall::EINVAL as isize)) as usize;
    };

    let seq = SYSCALL_DIAG_SEQ.fetch_add(1, Ordering::Relaxed);
    let should_log = seq < 32 || frame.nr == 59 || frame.nr == 60 || frame.nr == 231;

    if should_log {
        crate::kprintln!(
            "[diag][syscall] enter seq={} nr={} args=[{:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x}] k_rsp={:#x}",
            seq,
            frame.nr,
            frame.arg0,
            frame.arg1,
            frame.arg2,
            frame.arg3,
            frame.arg4,
            frame.arg5,
            read_syscall_kernel_rsp(),
        );
    }

    let ret = crate::syscall::dispatch(
        frame.nr,
        frame.arg0,
        frame.arg1,
        frame.arg2,
        frame.arg3,
        frame.arg4,
        frame.arg5,
    );

    if should_log {
        crate::kprintln!(
            "[diag][syscall] leave seq={} nr={} ret={:#x}",
            seq,
            frame.nr,
            ret,
        );
    }

    ret
}

unsafe extern "C" {
    fn syscall_entry();
}

pub unsafe fn init_syscall() {
    let kernel_cs = (KERNEL_CODE_SELECTOR & !0x3) as u64;

    // Intel/AMD 约定：64 位 SYSRET 的 CS 来自 STAR[63:48] + 16。
    // 当前返回路径使用 iretq，但仍写入兼容值。
    let user_sysret_base = ((USER_CODE_SELECTOR & !0x3) as u64).saturating_sub(16);

    let star = (kernel_cs << 32) | (user_sysret_base << 48);
    wrmsr(IA32_STAR, star);
    wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    wrmsr(IA32_FMASK, FMASK_CLEAR_FLAGS);

    let mut efer = rdmsr(IA32_EFER);
    efer |= EFER_SCE;
    wrmsr(IA32_EFER, efer);

    crate::kprintln!(
        "[diag][syscall] init STAR={:#x} LSTAR={:#x} FMASK={:#x}",
        star,
        syscall_entry as *const () as usize,
        FMASK_CLEAR_FLAGS,
    );
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
        frame.nr,
        frame.arg0,
        frame.arg1,
        frame.arg2,
        frame.arg3,
        frame.arg4,
        frame.arg5,
    )
}

global_asm!(
    r#"
    .section .text
    .global syscall_entry
    .type syscall_entry, @function
syscall_entry:
    cld

    // 在切换到内核栈之前，先把用户态关键信息保存在用户栈：
    // [r11(flags), rcx(rip), r15, r14, r13, r12, rbp, rbx, r9, r8, r10, rdx, rsi, rdi, rax(nr)]
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

    // r12: 用户保存帧基址
    mov r12, rsp

    // 切到预先武装的内核栈（由 enter_user_mode_iret 写入）
    mov rsp, qword ptr [rip + SYSCALL_KERNEL_RSP_SLOT]
    test rsp, rsp
    jnz 1f
    // 兜底：内核栈尚未武装时，继续使用当前栈，避免直接崩溃。
    mov rsp, r12
1:
    // r13: 保留用户保存帧基址
    mov r13, r12

    // SysV 调用对齐：call 前 rsp 16-byte 对齐
    and rsp, -16
    sub rsp, 64

    // 组装 RawSyscallFrame
    mov rax, [r13 + 112]
    mov [rsp + 0], rax
    mov rax, [r13 + 104]
    mov [rsp + 8], rax
    mov rax, [r13 + 96]
    mov [rsp + 16], rax
    mov rax, [r13 + 88]
    mov [rsp + 24], rax
    mov rax, [r13 + 80]
    mov [rsp + 32], rax
    mov rax, [r13 + 72]
    mov [rsp + 40], rax
    mov rax, [r13 + 64]
    mov [rsp + 48], rax

    lea rdi, [rsp]
    call syscall_dispatch_from_asm

    // 恢复用户寄存器（rax 保留 syscall 返回值）
    mov r15, [r13 + 16]
    mov r14, [r13 + 24]
    mov r12, [r13 + 40]
    mov rbp, [r13 + 48]
    mov rbx, [r13 + 56]
    mov r9,  [r13 + 64]
    mov r8,  [r13 + 72]
    mov r10, [r13 + 80]
    mov rdx, [r13 + 88]
    mov rsi, [r13 + 96]
    mov rdi, [r13 + 104]

    // iretq 返回帧：SS, RSP, RFLAGS, CS, RIP
    mov rcx, [r13 + 8]
    mov r11, [r13 + 0]

    push {user_data_sel}
    push qword ptr [r13 + 120]
    push r11
    push {user_code_sel}
    push rcx

    // r13 最后恢复
    mov r13, [r13 + 32]

    iretq
"#,
    user_code_sel = const USER_CODE_SELECTOR as u64,
    user_data_sel = const USER_DATA_SELECTOR as u64,
);
