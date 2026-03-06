# idt - 中断描述符表

`IDT` 与异常/IRQ 处理属于 x86_64 trap 域，当前代码位于：

- `kernel/src/interrupt/arch/x86_64/trap/idt.rs`
- `kernel/src/interrupt/arch/x86_64/trap/handlers.rs`

## API

```rust
pub unsafe fn init() -> Result<(), &'static str>;
pub unsafe fn load_idt();
pub unsafe fn get_idt_mut() -> &'static mut Idt;

pub fn enable_interrupts();
pub fn disable_interrupts();
pub fn interrupts_enabled() -> bool;
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R;
```

## 示例

```rust
use kernel::interrupt::arch::x86_64::trap::idt::{get_idt_mut, load_idt};

unsafe {
    let _idt = get_idt_mut();
    load_idt();
}
```

运行时调用方若只需要开关中断，应优先使用 façade：

```rust
use kernel::interrupt::{disable_interrupts, enable_interrupts, without_interrupts};
```

## 常用向量

```rust
pub const IRQ_BASE: u8 = 0x20;
pub const IRQ_TIMER: u8 = 0x20;
pub const IRQ_KEYBOARD: u8 = 0x21;
pub const IRQ_COM1: u8 = 0x24;
pub const IRQ_XHCI: u8 = 0x2B;

pub const DIVIDE_ERROR: u8 = 0;
pub const DOUBLE_FAULT: u8 = 8;
pub const GENERAL_PROTECTION: u8 = 13;
pub const PAGE_FAULT: u8 = 14;
```

## 相关文档

- [gdt - GDT](./gdt.md)
- [apic - APIC](./apic.md)
- [实现: IDT/异常处理](../../implementation/idt.md)
