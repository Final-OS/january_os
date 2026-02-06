.section .text
.global __switch

# __switch(current_task_cx_ptr: *mut TaskContext, next_task_cx_ptr: *const TaskContext)
# 
# 参数:
#   rdi: current_task_cx_ptr
#   rsi: next_task_cx_ptr
#
# TaskContext 结构:
#   rip: +0
#   rsp: +8
#   r15: +16
#   r14: +24
#   r13: +32
#   r12: +40
#   rbx: +48
#   rbp: +56

__switch:
    # 1. 保存当前任务上下文
    # 保存返回地址 (rip) 到 context
    movq (%rsp), %rax        # 栈顶是返回地址 (rip)
    movq %rax, 0(%rdi)       # current->rip = rip

    # 保存栈指针 (rsp)
    # 注意：此时 rsp 指向返回地址之后的位置
    # 我们保存的是 switch 调用完成后的 rsp (即弹出 rip 之后)
    leaq 8(%rsp), %rax       # rax = rsp + 8
    movq %rax, 8(%rdi)       # current->rsp = rax

    # 保存 callee-saved 寄存器
    movq %r15, 16(%rdi)
    movq %r14, 24(%rdi)
    movq %r13, 32(%rdi)
    movq %r12, 40(%rdi)
    movq %rbx, 48(%rdi)
    movq %rbp, 56(%rdi)

    # 2. 恢复下一个任务上下文
    # 恢复 callee-saved 寄存器
    movq 16(%rsi), %r15
    movq 24(%rsi), %r14
    movq 32(%rsi), %r13
    movq 40(%rsi), %r12
    movq 48(%rsi), %rbx
    movq 56(%rsi), %rbp

    # 恢复栈指针
    movq 8(%rsi), %rsp       # rsp = next->rsp

    # 恢复返回地址并跳转
    # 我们将 rip 压入栈，然后 ret
    movq 0(%rsi), %rax       # rax = next->rip
    pushq %rax               # push rip
    ret                      # pop rip and jump
