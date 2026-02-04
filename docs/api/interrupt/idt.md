# idt - 中断描述符表

IDT (Interrupt Descriptor Table) 定义中断和异常的处理程序。

## API

### IDT 操作

```rust
pub unsafe fn load_idt()
pub fn get_idt_mut() -> &'static mut Idt
```

**示例**：
```rust
use kernel::interrupt::idt::{get_idt_mut, load_idt};

// 设置处理程序
unsafe {
    let idt = get_idt_mut();
    idt.set_handler(0, IdtEntry::trap(handler as u64));
    load_idt();
}
```

### IDT 条目

```rust
pub struct IdtEntry;

impl IdtEntry {
    pub fn trap(handler_addr: u64) -> Self
    pub fn interrupt(handler_addr: u64) -> Self
    pub fn interrupt_ist(handler_addr: u64, ist_index: u8) -> Self
}
```

**类型**：
- `trap`: 陷阱门，禁用中断
- `interrupt`: 中断门，保持中断状态
- `interrupt_ist`: 中断门，使用 IST 栈

### 中断帧

```rust
#[repr(C)]
pub struct InterruptFrame {
    pub rip: u64,     // 指令指针
    pub cs: u64,      // 代码段
    pub rflags: u64,  // RFLAGS
    pub rsp: u64,     // 栈指针
    pub ss: u64,      // 栈段
}
```

## IDT 结构

```
IDT (256 个条目):
┌─────────────────────────────────────────────────────────┐
│  0-31: 异常                                             │
│    - Divide Error, Debug, NMI, Breakpoint, ...          │
│    - Page Fault, General Protection, Double Fault, ...  │
├─────────────────────────────────────────────────────────┤
│  32-255: IRQ 和系统调用                                 │
│    - 0x20: Timer (IRQ 0)                               │
│    - 0x21: Keyboard (IRQ 1)                            │
│    - 0x24: COM1 (IRQ 4)                               │
│    - 0x80: 系统调用 (规划)                             │
└─────────────────────────────────────────────────────────┘
```

## 异常向量

| 向量 | 异常 | 类型 | 处理程序 |
|------|------|------|----------|
| 0 | #DE | Fault | divide_error_handler |
| 1 | #DB | Fault/Trap | debug_handler |
| 2 | NMI | Interrupt | nmi_handler |
| 3 | #BP | Trap | breakpoint_handler |
| 4 | #OF | Trap | overflow_handler |
| 5 | #BR | Fault | bound_range_handler |
| 6 | #UD | Fault | invalid_opcode_handler |
| 7 | #NM | Fault | device_not_available_handler |
| 8 | #DF | Abort | double_fault_handler (IST1) |
| 10 | #TS | Fault | invalid_tss_handler |
| 11 | #NP | Fault | segment_not_present_handler |
| 12 | #SS | Fault | stack_fault_handler |
| 13 | #GP | Fault | general_protection_handler |
| 14 | #PF | Fault | page_fault_handler |
| 16 | #MF | Fault | x87_fpu_error_handler |
| 17 | #AC | Fault | alignment_check_handler |
| 18 | #MC | Abort | machine_check_handler |
| 19 | #XM | Fault | simd_exception_handler |
| 20 | #VE | Fault | virtualization_handler |
| 28 | #CP | Fault | control_protection_handler |

## 错误代码

某些异常会压入错误代码：

| 异常 | 有错误代码 | 错误代码含义 |
|------|-----------|--------------|
| #DF | 是 | 总是 0 |
| #TS | 是 | 选择子索引 |
| #NP | 是 | 选择子索引 |
| #SS | 是 | 错误代码 |
| #GP | 是 | 段选择子 / 0 |
| #PF | 是 | 页错误代码 |

## 页错误代码

```rust
pub struct PageErrorCode: u64 {
    const PRESENT = 1 << 0;   // 0 = 页不存在, 1 = 访问违规
    const WRITE   = 1 << 1;   // 0 = 读, 1 = 写
    const USER    = 1 << 2;   // 0 = 内核, 1 = 用户
    const RSVD    = 1 << 3;   // 保留位被设置
    const ID      = 1 << 4;   // 指令取
}
```

## 使用示例

### 自定义异常处理

```rust
use x86_64::structures::idt::InterruptStackFrame;

extern "x86-interrupt" fn my_custom_handler(
    frame: InterruptStackFrame)
{
    kprintln!("Custom exception!");
    kprintln!("RIP: {:#x}", frame.rip);
    kprintln!("RSP: {:#x}", frame.rsp);
}

// 注册处理程序
unsafe {
    let idt = get_idt_mut();
    idt.set_handler(0x30, IdtEntry::interrupt(my_custom_handler as u64));
    load_idt();
}
```

### 带错误码的处理

```rust
extern "x86-interrupt" fn my_fault_handler(
    frame: InterruptStackFrame,
    error_code: u64)
{
    kprintln!("Fault occurred!");
    kprintln!("Error code: {:#x}", error_code);
    kprintln!("RIP: {:#x}", frame.rip);
}

// 注册
idt.set_handler(0x31, IdtEntry::interrupt(my_fault_handler as u64));
```

### IST 处理

```rust
extern "x86-interrupt" fn critical_handler(
    frame: InterruptStackFrame)
{
    // 使用独立的 IST 栈，即使主栈损坏也能运行
    kprintln!("Critical handler on IST stack!");
    halt();
}

// 注册到 IST1
idt.set_handler(DOUBLE_FAULT,
    IdtEntry::interrupt_ist(critical_handler as u64, 1));
```

## IDT 描述符格式

```
64 位 IDT 描述符:

63                            47
├─────────────────────────────┤
│ Offset 63:32                │
├─────────────────────────────┤
│  │ P │ DPL │ 0 │ Type      │ │  Offset 31:16
│  │ 1 │ 00  │ 0 │ 1110 (Int) │
├─────────────────────────────┤
│ Offset 15:0                 │
└─────────────────────────────┘

P: Present (必须为 1)
DPL: Descriptor Privilege Level (0=内核, 3=用户)
Type:
  1110 = Interrupt Gate (中断门)
  1111 = Trap Gate (陷阱门)
```

## 中断门 vs 陷阱门

| 特性 | 中断门 | 陷阱门 |
|------|--------|--------|
| IF 标志 | 自动清零 (禁用中断) | 保持不变 |
| 用途 | 硬件中断 | 异常、系统调用 |

## 相关文档

- [gdt - GDT](./gdt.md)
- [apic - APIC](./apic.md)
- [handlers - 处理程序](./handlers.md)
