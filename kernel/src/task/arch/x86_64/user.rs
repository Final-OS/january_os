//! x86_64 用户态进入辅助（Batch 3 骨架）

use core::arch::asm;

use crate::interrupt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};

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
            "\x1b[90m[diag]\x1b[0m[user] arm syscall stack k_rsp={:#x} rip={:#x} rsp={:#x}",
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
