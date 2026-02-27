# interrupt - 中断子系统

中断子系统提供完整的中断管理功能，包括 GDT、IDT、APIC 和异常处理。

## API

### 初始化

```rust
pub unsafe fn init(info: &InterruptInitInfo) -> Result<(), &'static str>

pub struct InterruptInitInfo {
    pub kernel_stack_top: u64,
    pub local_apic_addr: u64,
    pub ioapic_addr: u64,
    pub ioapic_gsi_base: u32,
}
```

**示例**：
```rust
use kernel::interrupt::init;

let info = InterruptInitInfo {
    kernel_stack_top: 0xFFFF800000100000,
    local_apic_addr: 0xFEE00000,
    ioapic_addr: 0xFEC00000,
    ioapic_gsi_base: 0,
};

unsafe { init(&info)?; }
```

### 中断控制

```rust
pub fn enable_interrupts()
pub fn disable_interrupts()
pub fn interrupts_enabled() -> bool
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R
```

**示例**：
```rust
use kernel::interrupt::{enable_interrupts, disable_interrupts, without_interrupts};

// 启用中断
enable_interrupts();

// 禁用中断
disable_interrupts();

// 执行时禁用中断
let result = without_interrupts(|| {
    // 临界区代码
    42
});

// 检查状态
let enabled = interrupts_enabled();
```

### HLT 指令

```rust
pub fn halt()
pub fn halt_with_interrupts()
```

**示例**：
```rust
use kernel::interrupt::{halt, halt_with_interrupts};

// 等待中断（中断禁用）
unsafe { halt(); }

// 等待中断（中断启用）
halt_with_interrupts();
```

### 状态检查

```rust
pub fn initialized() -> bool
```

### 定时器

```rust
pub fn start_timer(frequency_hz: u32)
```

启动 APIC Timer。

**示例**：
```rust
use kernel::interrupt::start_timer;

// 100 Hz 定时器
start_timer(100);
```

### 重新导出

模块重新导出以下子模块的内容：

```rust
// GDT
pub use gdt::{
    KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR,
    USER_CODE_SELECTOR, USER_DATA_SELECTOR, TSS_SELECTOR,
    init_gdt, set_interrupt_stack,
};

// IDT
pub use idt::{
    IdtEntry, GateType, InterruptFrame,
    IRQ_TIMER, IRQ_KEYBOARD, IRQ_COM1,
    DIVIDE_ERROR, PAGE_FAULT, GENERAL_PROTECTION,
    // ...
};

// APIC
pub use apic::{
    init_local_apic, init_ioapic, init_apic_timer,
    local_apic_eoi, local_apic_id, calibrate_timer,
    ioapic_set_irq, ioapic_mask_irq, ioapic_unmask_irq,
    // ...
};

// PIT
pub use pit::{
    pit_set_frequency, pit_get_ticks, PIT_FREQUENCY,
};

// Handlers
pub use handlers::{
    timer_ticks, set_timer_debug,
};

// Input
pub use crate::drivers::input::{
    read_char, has_char, buffer_len,
    is_shift_pressed, is_ctrl_pressed,
};
```

## 中断向量

### 异常向量 (0-31)

| 向量 | 异常 | 处理程序 |
|------|------|----------|
| 0 | #DE | divide_error_handler |
| 1 | #DB | debug_handler |
| 2 | NMI | nmi_handler |
| 3 | #BP | breakpoint_handler |
| 8 | #DF | double_fault_handler (IST1) |
| 13 | #GP | general_protection_handler |
| 14 | #PF | page_fault_handler |

### IRQ 向量 (32+)

| 向量 | IRQ | 设备 |
|------|-----|------|
| 0x20 | 0 | PIT/Timer |
| 0x21 | 1 | PS/2 键盘 |
| 0x24 | 4 | COM1 串口 |

## 使用示例

### 自定义中断处理

```rust
use kernel::interrupt::{IdtEntry, ioapic_set_irq, ioapic_unmask_irq};
use x86_64::structures::idt::InterruptStackFrame;

// 自定义处理程序
extern "x86-interrupt" fn my_irq_handler(_frame: InterruptStackFrame) {
    kprintln!("My IRQ handler!");

    // 处理中断...

    // 发送 EOI
    local_apic_eoi();
}

// 注册处理程序
unsafe {
    let idt = idt::get_idt_mut();
    idt.set_handler(0x40, IdtEntry::interrupt(my_irq_handler as u64));
}

// 设置 IRQ 重映射
ioapic_set_irq(5, 0x40, 0, false, false);
ioapic_unmask_irq(5);
```

### 中断安全代码

```rust
use kernel::interrupt::without_interrupts;
use core::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn increment_counter() {
    // 在临界区内完成更新
    without_interrupts(|| {
        COUNTER.fetch_add(1, Ordering::Relaxed);
    });
}
```

## 常量

```rust
// GDT 选择子
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x18;
pub const USER_DATA_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

// IRQ 向量
pub const IRQ_BASE: u8 = 0x20;
pub const IRQ_TIMER: u8 = 0x20;
pub const IRQ_KEYBOARD: u8 = 0x21;
pub const IRQ_COM1: u8 = 0x24;
pub const IRQ_SPURIOUS: u8 = 0xFF;

// 异常向量
pub const DIVIDE_ERROR: u8 = 0;
pub const DEBUG: u8 = 1;
pub const NMI: u8 = 2;
pub const BREAKPOINT: u8 = 3;
pub const OVERFLOW: u8 = 4;
pub const BOUND_RANGE: u8 = 5;
pub const INVALID_OPCODE: u8 = 6;
pub const DEVICE_NOT_AVAILABLE: u8 = 7;
pub const DOUBLE_FAULT: u8 = 8;
pub const INVALID_TSS: u8 = 10;
pub const SEGMENT_NOT_PRESENT: u8 = 11;
pub const STACK_FAULT: u8 = 12;
pub const GENERAL_PROTECTION: u8 = 13;
pub const PAGE_FAULT: u8 = 14;
pub const X87_FPU_ERROR: u8 = 16;
pub const ALIGNMENT_CHECK: u8 = 17;
pub const MACHINE_CHECK: u8 = 18;
pub const SIMD_EXCEPTION: u8 = 19;
pub const VIRTUALIZATION: u8 = 20;
pub const CONTROL_PROTECTION: u8 = 28;
```

## 相关文档

- [gdt - GDT](./gdt.md)
- [idt - IDT](./idt.md)
- [apic - APIC](./apic.md)
- [pit - PIT](./pit.md)
- [handlers - 处理程序](./handlers.md)
