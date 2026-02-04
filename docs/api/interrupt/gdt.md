# gdt - 全局描述符表

GDT (Global Descriptor Table) 定义内存段和任务状态段 (TSS)。

## API

### 初始化

```rust
pub unsafe fn init_gdt(kernel_stack_top: u64)
```

初始化 GDT 和 TSS。

**参数**：
- `kernel_stack_top`: 内核栈顶地址

**示例**：
```rust
use kernel::interrupt::init_gdt;

unsafe {
    init_gdt(0xFFFF800000100000);
}
```

### 中断栈

```rust
pub unsafe fn set_interrupt_stack(ist_index: u8, stack_top: u64)
```

设置 IST (Interrupt Stack Table) 栈。

**参数**：
- `ist_index`: IST 索引 (1-7)
- `stack_top`: 栈顶地址

**示例**：
```rust
use kernel::interrupt::set_interrupt_stack;

unsafe {
    // 为 Double Fault 设置 IST1
    set_interrupt_stack(1, 0xFFFF800002000000);
}
```

## 选择子

```rust
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x18;
pub const USER_DATA_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;
```

## GDT 结构

```
GDT 条目:
┌─────────────────────────────────────────────────────────┐
│  空描述符 (索引 0)                                      │
├─────────────────────────────────────────────────────────┤
│  内核代码段 (索引 1, 选择子 0x08)                       │
│  - 64 位代码段                                         │
│  - DPL = 0 (内核)                                      │
├─────────────────────────────────────────────────────────┤
│  内核数据段 (索引 2, 选择子 0x10)                       │
│  - 读写数据段                                         │
│  - DPL = 0 (内核)                                      │
├─────────────────────────────────────────────────────────┤
│  用户代码段 (索引 3, 选择子 0x18)                       │
│  - 64 位代码段                                         │
│  - DPL = 3 (用户)                                      │
├─────────────────────────────────────────────────────────┤
│  用户数据段 (索引 4, 选择子 0x20)                       │
│  - 读写数据段                                         │
│  - DPL = 3 (用户)                                      │
├─────────────────────────────────────────────────────────┤
│  TSS (索引 5, 选择子 0x28)                             │
│  - 任务状态段                                         │
│  - 包含 IST 栈指针                                     │
└─────────────────────────────────────────────────────────┘
```

## TSS 结构

```rust
#[repr(C, packed)]
pub struct TaskStateSegment {
    pub reserved0: u32,
    pub rsp0: u64,        // 特权级 0 栈
    pub rsp1: u64,        // 特权级 1 栈
    pub rsp2: u64,        // 特权级 2 栈
    pub reserved1: u64,
    pub ist1: u64,        // IST 1 栈 (Double Fault)
    pub ist2: u64,        // IST 2 栈
    pub ist3: u64,        // IST 3 栈
    pub ist4: u64,        // IST 4 栈
    pub ist5: u64,        // IST 5 栈
    pub ist6: u64,        // IST 6 栈
    pub ist7: u64,        // IST 7 栈
    // ...
}
```

## IST (Interrupt Stack Table)

| IST 索引 | 用途 |
|----------|------|
| IST 1 | Double Fault (#DF) |
| IST 2 | NMI |
| IST 3 | Machine Check (#MC) |
| IST 4-7 | 保留或自定义 |

**为什么需要 IST**：

当发生栈错误（如栈溢出）时，使用正常的栈会导致双故障。IST 提供独立的栈确保即使主栈损坏也能处理异常。

## 段描述符

```rust
pub struct SegmentDescriptor {
    pub limit_low: u16,
    pub base_low: u16,
    pub base_middle: u8,
    pub access: u8,
    pub limit_high: u8,
    pub base_high: u8,
}
```

**访问字节**：

```
7 6 5 4 3 2 1 0
│ │ │ │ │ │ │ └─ Accessed (A)
│ │ │ │ │ │ └─── Readable/Writable (R/W)
│ │ │ │ │ └───── Conforming (C) / Direction (D)
│ │ │ │ └─────── Code (C) / Data (E)
│ │ │ └───────── DPL (Descriptor Privilege Level)
│ │ └─────────── Present (P)
│ └───────────── S (System flag)
└─────────────── AVL / L / D/B
```

## 使用示例

### 创建自定义段

```rust
use kernel::interrupt::gdt::{SegmentDescriptor, GDT};

// 创建 64 位代码段
let code_desc = SegmentDescriptor::new_code_segment(
    0,      // 基址
    0xFFFFF,// 限制
    0,      // DPL (内核)
);

// 添加到 GDT
let index = GDT.lock().add_entry(code_desc);
let selector = (index << 3) as u16;
```

### 段选择子格式

```
15  3 2   0
┌────┬────┐
│TI │RPL │
└────┴────┘

TI: Table Indicator
  0 = GDT
  1 = LDT

RPL: Requestor Privilege Level
  0 = 内核
  3 = 用户

示例:
  0x08 = GDT 索引 1, RPL 0 (内核代码)
  0x18 = GDT 索引 3, RPL 3 (用户代码)
```

## 相关文档

- [idt - IDT](./idt.md)
- [实现: GDT/TSS](../../implementation/gdt.md)
