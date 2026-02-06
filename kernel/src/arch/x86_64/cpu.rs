use core::arch::asm;

/// 获取当前栈顶地址
///
/// 注意：这只是获取当前 RSP 并按页对齐，作为内核栈顶的近似值。
/// 在 x86_64 中，栈向下增长，所以栈顶是高地址。
pub fn current_stack_top() -> u64 {
    let rsp: u64;
    unsafe {
        asm!("mov {}, rsp", out(reg) rsp);
    }
    // 假设栈大小至少为 4KB，且按 4KB 对齐
    (rsp + 0xFFF) & !0xFFF
}

/// 挂起 CPU
pub fn halt() {
    unsafe {
        asm!("hlt");
    }
}
