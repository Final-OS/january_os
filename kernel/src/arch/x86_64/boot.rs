use alloc::alloc::{alloc, Layout};

const RUNTIME_BOOT_STACK_SIZE: usize = 64 * 1024;
const RUNTIME_BOOT_STACK_ALIGN: usize = 4096;

#[inline(never)]
pub unsafe fn switch_to_runtime_boot_stack(
    entry: extern "C" fn(*const u8, usize) -> !,
    init_cmd: &str,
) -> ! {
    let layout = Layout::from_size_align(RUNTIME_BOOT_STACK_SIZE, RUNTIME_BOOT_STACK_ALIGN)
        .expect("runtime boot stack layout");
    let stack_base = alloc(layout);
    if stack_base.is_null() {
        panic!("runtime boot stack alloc failed");
    }

    let stack_top = stack_base as usize + RUNTIME_BOOT_STACK_SIZE;
    let stack_top_aligned = stack_top & !0xFusize;

    core::arch::asm!(
        "mov rsp, {stack}",
        "xor rbp, rbp",
        "mov rdi, {arg0}",
        "mov rsi, {arg1}",
        "jmp {entry}",
        stack = in(reg) stack_top_aligned,
        arg0 = in(reg) init_cmd.as_ptr(),
        arg1 = in(reg) init_cmd.len(),
        entry = in(reg) entry as usize,
        options(noreturn)
    );
}
