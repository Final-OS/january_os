//! AArch64 架构占位实现（组件化宏内核结构骨架）
//!
//! 说明：
//! - 当前仓库运行主线仍是 x86_64。
//! - 本模块用于提前固定多架构目录与接口形状，便于后续逐步补齐实现。

/// 获取当前栈顶地址（占位）。
pub fn current_stack_top() -> u64 {
    0
}

pub unsafe fn switch_to_runtime_boot_stack(
    entry: extern "C" fn(*const u8, usize) -> !,
    init_cmd: &str,
) -> ! {
    entry(init_cmd.as_ptr(), init_cmd.len())
}

/// 挂起 CPU（占位）。
pub fn halt() {
    loop {
        core::hint::spin_loop();
    }
}

/// 关机（占位）。
pub fn shutdown() -> ! {
    loop {
        halt();
    }
}

/// 重启（占位）。
pub fn reboot() -> ! {
    loop {
        halt();
    }
}

/// 端口 I/O 占位（AArch64 无 x86 端口 I/O 语义）。
#[inline]
pub unsafe fn outb(_port: u16, _value: u8) {}
#[inline]
pub unsafe fn outw(_port: u16, _value: u16) {}
#[inline]
pub unsafe fn outl(_port: u16, _value: u32) {}
#[inline]
pub unsafe fn inb(_port: u16) -> u8 {
    0
}
#[inline]
pub unsafe fn inw(_port: u16) -> u16 {
    0
}
#[inline]
pub unsafe fn inl(_port: u16) -> u32 {
    0
}

/// 串口输出占位。
pub fn serial_init() {}
pub fn serial_print(_msg: &str) {}
pub fn serial_println(_msg: &str) {}

/// syscall 入口占位（接口形状预留）。
pub mod syscall {
    pub unsafe fn init_syscall() {}
    pub fn set_syscall_kernel_rsp(_rsp: u64) {}
}
