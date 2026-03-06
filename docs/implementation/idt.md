# IDT/异常处理

中断描述符表 (IDT) 定义 CPU 如何响应中断和异常。

## 文件

- `kernel/src/interrupt/arch/x86_64/trap/idt.rs` - IDT 管理
- `kernel/src/interrupt/arch/x86_64/trap/handlers.rs` - 异常处理程序

## IDT 结构

```
IDT (256 个条目)
┌─────────────────────────────────────────────────────────┐
│  0-31: 异常 (CPU 异常)                               │
│    - Divide Error, Page Fault, General Protection, ... │
├─────────────────────────────────────────────────────────┤
│  32-255: IRQ 和系统调用                               │
│    - 0x20: Timer (IRQ 0)                              │
│    - 0x21: Keyboard (IRQ 1)                           │
│    - 0x24: COM1 (IRQ 4)                               │
│    - 0x80: 系统调用 (规划)                             │
└─────────────────────────────────────────────────────────┘
```

## IDT 条目类型

### 中断门 (Interrupt Gate)

```rust
IdtEntry::interrupt(handler_addr as u64)
```

- 禁用中断进入
- 用于硬件中断
- 自动恢复中断标志

### 陷阱门 (Trap Gate)

```rust
IdtEntry::trap(handler_addr as u64)
```

- 不改变中断标志
- 用于异常和系统调用
- 保持中断状态

### 中断门 with IST

```rust
IdtEntry::interrupt_ist(handler_addr as u64, ist_index)
```

- 使用指定的 IST 栈
- 用于严重异常（如 Double Fault）

## IDT 初始化

```rust
pub unsafe fn init_idt() -> Result<(), &'static str>
```

**步骤**:
1. 设置异常处理程序
2. 设置 IRQ 处理程序
3. 加载 IDT

```rust
let idt = idt::get_idt_mut();

// 设置异常处理
idt.set_handler(DIVIDE_ERROR, IdtEntry::trap(divide_error_handler as u64));
idt.set_handler(PAGE_FAULT, IdtEntry::trap(page_fault_handler as u64));
idt.set_handler(DOUBLE_FAULT, IdtEntry::interrupt_ist(double_fault_handler as u64, 1));

// 设置 IRQ 处理
idt.set_handler(IRQ_TIMER, IdtEntry::interrupt(timer_handler as u64));
idt.set_handler(IRQ_KEYBOARD, IdtEntry::interrupt(keyboard_handler as u64));

// 加载 IDT
idt::load_idt();
```

## 中断帧

```rust
#[repr(C)]
pub struct InterruptFrame {
    pub rip: u64,     // 指令指针
    pub cs: u64,      // 代码段选择子
    pub rflags: u64,  // RFLAGS 寄存器
    pub rsp: u64,     // 栈指针
    pub ss: u64,      // 栈段选择子
}
```

## 异常处理

### Divide Error (#DE)

```rust
extern "x86-interrupt" fn divide_error_handler(frame: InterruptFrame) {
    kprintln!("!!! DIVIDE ERROR !!!");
    kprintln!("  RIP: {:#x}", frame.rip);
    halt();
}
```

### Page Fault (#PF)

```rust
extern "x86-interrupt" fn page_fault_handler(
    frame: InterruptFrame,
    error_code: u64
) {
    let fault_addr = unsafe { read_cr2() };

    // 解析错误代码
    let present = error_code & 0x01 != 0;
    let write = error_code & 0x02 != 0;
    let user = error_code & 0x04 != 0;

    // 处理页错误...
}
```

### Double Fault (#DF)

```rust
extern "x86-interrupt" fn double_fault_handler(
    frame: InterruptFrame,
    error_code: u64
) {
    // 使用 IST1 栈，即使主栈损坏也能运行
    kprintln!("!!! DOUBLE FAULT !!!");
    halt();  // 无法恢复
}
```

## 异常向量

| 向量 | 异常 | 处理程序 | IST |
|------|------|----------|-----|
| 0 | #DE | divide_error_handler | - |
| 1 | #DB | debug_handler | - |
| 2 | NMI | nmi_handler | - |
| 3 | #BP | breakpoint_handler | - |
| 8 | #DF | double_fault_handler | IST1 |
| 13 | #GP | general_protection_handler | - |
| 14 | #PF | page_fault_handler | - |
| 18 | #MC | machine_check_handler | IST3 |

## 使用示例

### 自定义异常处理

```rust
use x86_64::structures::idt::InterruptStackFrame;

extern "x86-interrupt" fn my_overflow_handler(
    frame: InterruptFrame)
{
    kprintln!("Overflow occurred at {:#x}", frame.rip);
}

// 注册
unsafe {
    let idt = idt::get_idt_mut();
    idt.set_handler(OVERFLOW, IdtEntry::trap(my_overflow_handler as u64));
    idt::load_idt();
}
```

### 页错误处理

```rust
extern "x86-interrupt" fn custom_page_fault(
    frame: InterruptFrame,
    error_code: u64
) {
    let fault_addr = unsafe { read_cr2() };
    let virt_addr = VirtAddr::new(fault_addr);

    // 检查地址是否在 VMA 中
    let mm = get_init_mm();
    if let Some(vma) = mm.find_vma(virt_addr) {
        // 处理缺页、COW 等
        handle_vma_fault(vma, virt_addr, error_code);
    } else {
        // 无效访问
        kprintln!("Invalid access: {:#x}", fault_addr);
    };
}
```

## 相关文档

- [API: idt](../api/interrupt/idt.md)
- [API: handlers](../api/interrupt/handlers.md)
- [GDT/TSS](./gdt.md)
- [APIC](./apic.md)
