//! x86_64 用户态进入辅助（Batch 3 骨架）

use core::arch::asm;

use crate::interrupt::arch::x86_64::entry::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};

/// 用户态入口帧（用于 iretq 切换）
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct UserEnterFrame {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub cs: u64,
    pub ss: u64,
}

impl UserEnterFrame {
    pub const fn new(rip: u64, rsp: u64) -> Self {
        Self {
            rip,
            rsp,
            rflags: 0x202,
            cs: USER_CODE_SELECTOR as u64,
            ss: USER_DATA_SELECTOR as u64,
        }
    }
}

/// `fork` 子进程从 syscall 返回路径恢复到用户态时使用的寄存器帧。
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ForkReturnFrame {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r9: u64,
    pub r8: u64,
    pub r10: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
}

#[inline]
pub fn build_user_enter_frame(entry: u64, stack_top: u64) -> UserEnterFrame {
    UserEnterFrame::new(entry, stack_top)
}

/// 通过 iretq 进入 ring3。
///
/// 当前用于用户态 `execve` 切换路径。
pub unsafe fn enter_user_mode_iret(frame: &UserEnterFrame) -> ! {
    let frame_rip = frame.rip;
    let frame_rsp = frame.rsp;
    let frame_rflags = frame.rflags;
    let frame_cs = frame.cs;
    let frame_ss = frame.ss;

    let kernel_rsp: u64;
    asm!(
        "mov {}, rsp",
        out(reg) kernel_rsp,
        options(nostack, preserves_flags)
    );

    crate::arch::syscall::set_syscall_kernel_rsp(kernel_rsp);
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[user] set syscall stack k_rsp={:#x} rip={:#x} rsp={:#x}",
            kernel_rsp,
            frame_rip,
            frame_rsp,
        );
    }

    asm!(
        "push {ss}",
        "push {rsp}",
        "push {rflags}",
        "push {cs}",
        "push {rip}",
        "iretq",
        ss = in(reg) frame_ss,
        rsp = in(reg) frame_rsp,
        rflags = in(reg) frame_rflags,
        cs = in(reg) frame_cs,
        rip = in(reg) frame_rip,
        options(noreturn),
    );
}

/// 以“syscall 返回后继续执行”的寄存器状态恢复到 ring3。
pub unsafe fn enter_user_fork_return(frame: &ForkReturnFrame) -> ! {
    let kernel_rsp: u64;
    asm!(
        "mov {}, rsp",
        out(reg) kernel_rsp,
        options(nostack, preserves_flags)
    );

    crate::arch::syscall::set_syscall_kernel_rsp(kernel_rsp);

    let frame_ptr = frame as *const ForkReturnFrame as u64;
    asm!(
        "mov r15, [rdi + 24]",
        "mov r14, [rdi + 32]",
        "mov r13, [rdi + 40]",
        "mov r12, [rdi + 48]",
        "mov rbp, [rdi + 56]",
        "mov rbx, [rdi + 64]",
        "mov r9,  [rdi + 72]",
        "mov r8,  [rdi + 80]",
        "mov r10, [rdi + 88]",
        "mov rdx, [rdi + 96]",
        "mov rsi, [rdi + 104]",
        "mov rax, [rdi + 120]",
        "mov rcx, [rdi + 0]",
        "mov r11, [rdi + 16]",
        "push {user_ss}",
        "push qword ptr [rdi + 8]",
        "push r11",
        "push {user_cs}",
        "push rcx",
        "mov rdi, [rdi + 112]",
        "iretq",
        in("rdi") frame_ptr,
        user_cs = const USER_CODE_SELECTOR as u64,
        user_ss = const USER_DATA_SELECTOR as u64,
        options(noreturn),
    );
}
