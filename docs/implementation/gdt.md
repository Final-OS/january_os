# GDT/TSS

全局描述符表 (GDT) 和任务状态段 (TSS) 是 x86_64 架构的基础内存保护机制。

## 文件

- `kernel/src/interrupt/arch/x86_64/gdt.rs`

## GDT 结构

```
GDT (在内存中)
┌─────────────────────────────────────────────────────────┐
│  选择子 0x00: 空描述符                                │
├─────────────────────────────────────────────────────────┤
│  选择子 0x08: 内核代码段 (64-bit, DPL=0)                │
├─────────────────────────────────────────────────────────┤
│  选择子 0x10: 内核数据段 (DPL=0)                       │
├─────────────────────────────────────────────────────────┤
│  选择子 0x18: 用户代码段 (64-bit, DPL=3)                │
├─────────────────────────────────────────────────────────┤
│  选择子 0x20: 用户数据段 (DPL=3)                       │
├─────────────────────────────────────────────────────────┤
│  选择子 0x28: TSS (包含 IST)                            │
└─────────────────────────────────────────────────────────┘
```

## GDT 初始化

```rust
pub unsafe fn init_gdt(kernel_stack_top: u64)
```

### 创建 GDT

```rust
use core::cell::UnsafeCell;

// 5 个段 + TSS
const GDT_ENTRIES: usize = 6;

struct GdtState {
    inner: UnsafeCell<[GdtEntry; GDT_ENTRIES]>,
}

unsafe impl Sync for GdtState {}

impl GdtState {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new([GdtEntry::NULL; GDT_ENTRIES]),
        }
    }
}

static GDT: GdtState = GdtState::new();

fn gdt_ref() -> &'static [GdtEntry; GDT_ENTRIES] {
    unsafe { &*GDT.inner.get() }
}

fn gdt_mut() -> &'static mut [GdtEntry; GDT_ENTRIES] {
    unsafe { &mut *GDT.inner.get() }
}
```

### 设置段描述符

```rust
let gdt = gdt_mut();

// 内核代码段
gdt[1] = GdtEntry::new_code_segment(
    0,           // 基址
    0xFFFFF800,  // 限制 (实际被忽略)
    0,           // DPL (内核)
);

// 内核数据段
gdt[2] = GdtEntry::new_data_segment(
    0,
    0xFFFFF800,
    0,
);

// 用户代码段
gdt[3] = GdtEntry::new_code_segment(
    0,
    0xFFFFF800,
    3,           // DPL (用户)
);

// 用户数据段
gdt[4] = GdtEntry::new_data_segment(
    0,
    0xFFFFF800,
    3,
);
```

### TSS 设置

```rust
pub unsafe fn set_interrupt_stack(ist_index: u8, stack_top: u64)
```

**TSS 结构**:
```rust
#[repr(C, packed)]
pub struct TaskStateSegment {
    pub reserved0: u32,
    pub rsp0: u64,        // 特权级 0 栈
    pub rsp1: u64,        // 特权级 1 栈
    pub rsp2: u64,        // 特权级 2 栈
    pub reserved1: u64,
    pub ist1: u64,        // IST 1 栈 (Double Fault)
    pub ist2: u64,        // IST 2 栈 (NMI)
    pub ist3: u64,        // IST 3 栈 (Machine Check)
    pub ist4: u64,        // IST 4 栈
    pub ist5: u64,        // IST 5 栈
    pub ist6: u64,        // IST 6 栈
    pub ist7: u64,        // IST 7 栈
    pub reserved2: u16,
    pub iomap_base: u64,
    pub iomap_base_h: u32,
    pub reserved3: u32,
    pub reserved4: u64,
}
```

### 加载 GDT

```rust
unsafe fn load_gdt() {
    let gdt = gdt_ref();
    let gdt_ptr = GdtPointer {
        limit: (GDT_ENTRIES * 16 - 1) as u16,
        base: gdt.as_ptr() as u64,
    };

    asm!("lgdt [{}]", in(reg) &gdt_ptr);

    // 重新加载段寄存器
    asm!(
        "mov ax, 0x10",  // 内核数据段
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "mov ss, ax",
    );
}
```

## IST (中断栈表)

IST 提供独立的栈用于处理严重异常，避免栈损坏时的双故障。

| IST 索引 | 用途 |
|----------|------|
| IST 1 | Double Fault (#DF) |
| IST 2 | NMI |
| IST 3 | Machine Check (#MC) |
| IST 4-7 | 保留或自定义 |

## 段选择子格式

```
15  3 2   0
┌────┬────┐
│TI │RPL │
└────┴────┘

TI: Table Indicator (0 = GDT)
RPL: Requestor Privilege Level (0 = 内核, 3 = 用户)

索引 = 选择子 >> 3
```

## 使用示例

### 获取内核栈并设置 TSS

```rust
use kernel::interrupt::{init_gdt, set_interrupt_stack};

// 获取当前栈顶
let rsp: u64;
unsafe { asm!("mov {}, rsp", out(reg) rsp); }
let kernel_stack_top = (rsp + 0xFFF) & !0xFFF;

// 初始化 GDT
unsafe { init_gdt(kernel_stack_top); }

// 为 Double Fault 设置 IST1
let ist1_page = alloc_pages(2, GFP_KERNEL).unwrap();
let ist1_top = direct_map + page_to_pfn(ist1_page) * 4096 + 16 * 1024;
unsafe { set_interrupt_stack(1, ist1_top); }
```

## 相关文档

- [API: gdt](../api/interrupt/gdt.md)
- [IDT](./idt.md)
- [引导流程](./boot.md)
