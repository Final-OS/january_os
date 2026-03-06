# gdt - 全局描述符表

`GDT/TSS` 是 x86_64 专有的入口表实现，当前代码位于 `kernel/src/interrupt/arch/x86_64/entry/gdt.rs`。

## API

```rust
pub unsafe fn init_gdt(cpu_id: usize, kernel_stack_top: u64);
pub unsafe fn set_interrupt_stack(cpu_id: usize, ist_index: u8, stack_top: u64);

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x18 | 3;
pub const USER_DATA_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;
```

## 示例

```rust
use kernel::interrupt::arch::x86_64::entry::gdt::{init_gdt, set_interrupt_stack};

unsafe {
    init_gdt(0, 0xFFFF800000100000);
    set_interrupt_stack(0, 1, 0xFFFF800002000000);
}
```

## 说明

- 每个 CPU 都会初始化自己的 GDT/TSS
- `IST1` 主要用于 Double Fault 等严重异常
- 该模块不再从 `kernel::interrupt::gdt` 顶层路径导出

## 相关文档

- [idt - IDT](./idt.md)
- [实现: GDT/TSS](../../implementation/gdt.md)
