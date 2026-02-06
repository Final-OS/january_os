//! Context switch assembly

use core::arch::global_asm;

global_asm!(r#"
    .section .text
    .global __switch
    .type __switch, @function
__switch:
    # rdi: *mut usize (current_task_cx_ptr)
    # rsi: *const usize (next_task_cx_ptr)

    # Save callee-saved registers
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    # Switch stack pointer
    # [rdi] = rsp
    mov [rdi], rsp
    # rsp = [rsi]
    mov rsp, [rsi]

    # Restore callee-saved registers
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp

    ret
"#);

unsafe extern "C" {
    /// Switch context from `current_task_cx_ptr` to `next_task_cx_ptr`.
    /// 
    /// # Arguments
    /// * `current_task_cx_ptr` - Pointer to where to save the current stack pointer
    /// * `next_task_cx_ptr` - Pointer to where to load the next stack pointer from
    pub fn __switch(current_task_cx_ptr: *mut usize, next_task_cx_ptr: *const usize);
}
