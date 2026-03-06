# handlers - 中断和异常处理程序

handlers 模块提供所有中断和异常的默认处理程序。

## API

### 定时器

```rust
pub fn timer_ticks() -> u64
pub fn set_timer_debug(enabled: bool)
```

**定时器计数**：
```rust
use kernel::interrupt::timer_ticks;

let ticks = timer_ticks();
kprintln!("Uptime: {} ticks", ticks);
```

**调试输出**：
```rust
use kernel::interrupt::diag::set_timer_debug;

set_timer_debug(true);  // 每个 tick 打印信息
```

### 输入

从 `drivers::input` 重新导出：

```rust
pub use crate::drivers::input::{
    read_char,       // 读取字符
    has_char,        // 检查是否有字符
    buffer_len,      // 缓冲区长度
    last_scancode,   // 最后扫描码
    last_char,       // 最后字符
    is_shift_pressed,  // Shift 状态
    is_ctrl_pressed,   // Ctrl 状态
    is_alt_pressed,    // Alt 状态
};
```

**示例**：
```rust
use kernel::interrupt::{
    read_char, has_char,
    is_shift_pressed, is_ctrl_pressed,
};

// 检查输入
while let Some(c) = read_char() {
    // 修饰键
    let shift = is_shift_pressed();
    let ctrl = is_ctrl_pressed();

    // 处理字符
    if ctrl && c == b'c' {
        kprintln!("Ctrl+C");
    }
}
```

## 处理程序列表

### 异常处理程序

| 函数 | 向量 | 异常 |
|------|------|------|
| `divide_error_handler` | 0 | #DE |
| `debug_handler` | 1 | #DB |
| `nmi_handler` | 2 | NMI |
| `breakpoint_handler` | 3 | #BP |
| `overflow_handler` | 4 | #OF |
| `bound_range_handler` | 5 | #BR |
| `invalid_opcode_handler` | 6 | #UD |
| `device_not_available_handler` | 7 | #NM |
| `double_fault_handler` | 8 | #DF |
| `invalid_tss_handler` | 10 | #TS |
| `segment_not_present_handler` | 11 | #NP |
| `stack_fault_handler` | 12 | #SS |
| `general_protection_handler` | 13 | #GP |
| `page_fault_handler` | 14 | #PF |
| `x87_fpu_error_handler` | 16 | #MF |
| `alignment_check_handler` | 17 | #AC |
| `machine_check_handler` | 18 | #MC |
| `simd_exception_handler` | 19 | #XM |
| `virtualization_handler` | 20 | #VE |
| `control_protection_handler` | 28 | #CP |

### IRQ 处理程序

| 函数 | 向量 | IRQ | 设备 |
|------|------|-----|------|
| `timer_handler` | 0x20 | 0 | APIC Timer |
| `keyboard_handler` | 0x21 | 1 | PS/2 键盘 |
| `serial_handler` | 0x24 | 4 | COM1 |
| `spurious_handler` | 0xFF | - | Spurious IRQ |

## 使用示例

### 自定义异常处理

```rust
use x86_64::structures::idt::InterruptStackFrame;

extern "x86-interrupt" fn custom_divide_error(
    frame: InterruptStackFrame)
{
    kprintln!("!!! DIVIDE ERROR !!!");
    kprintln!("RIP: {:#x}", frame.rip);
    kprintln!("RSP: {:#x}", frame.rsp);

    // 处理或停止...
    loop { unsafe { core::arch::asm!("hlt"); } }
}

// 在 init_idt() 中注册
idt.set_handler(DIVIDE_ERROR,
    IdtEntry::trap(custom_divide_error as u64));
```

### 自定义 IRQ 处理

```rust
use x86_64::structures::idt::InterruptStackFrame;
use kernel::interrupt::local_apic_eoi;
use core::sync::atomic::{AtomicU64, Ordering};

extern "x86-interrupt" fn custom_timer_handler(
    _frame: InterruptStackFrame)
{
    static COUNT: AtomicU64 = AtomicU64::new(0);

    let count = COUNT.fetch_add(1, Ordering::Relaxed) + 1;

    if count % 100 == 0 {
        kprintln!("Timer: {}", count);
    }

    // 发送 EOI
    local_apic_eoi();
}

// 注册
idt.set_handler(IRQ_TIMER,
    IdtEntry::interrupt(custom_timer_handler as u64));
```

### 页错误处理

```rust
extern "x86-interrupt" fn custom_page_fault(
    frame: InterruptStackFrame,
    error_code: u64)
{
    let fault_addr = unsafe { x86_64::registers::control::Cr2::read() };

    kprintln!("!!! PAGE FAULT !!!");
    kprintln!("Address: {:#x}", fault_addr.as_u64());
    kprintln!("Error: {:#x}", error_code);
    kprintln!("RIP: {:#x}", frame.rip);
    kprintln!("RSP: {:#x}", frame.rsp);

    // 解析错误代码
    let present = error_code & 0x01 != 0;
    let write = error_code & 0x02 != 0;
    let user = error_code & 0x04 != 0;

    kprintln!("Access: {} {} {}",
        if user { "user" } else { "kernel" },
        if write { "write" } else { "read" },
        if present { "violation" } else { "not present" });
}
```

## 调试

### Panic 输出

```rust
!!! KERNEL PANIC !!!
  kernel/src/main.rs:42
  attempting to divide by zero

!!! DOUBLE FAULT !!!
  RIP: 0xffffffff80001000
  RSP: 0xffffffff80002000

!!! PAGE FAULT !!!
  Address: 0xdeadbeef
  Error: 0x04 (user, read, not present)
  RIP: 0x40000000
```

### 调试信息

```rust
use kernel::interrupt::diag::set_timer_debug;

set_timer_debug(true);

// 输出:
// [Timer] tick=100
// [Timer] tick=101
// [Timer] tick=102
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

## 注意事项

1. **中断安全**：处理程序中不能使用可能睡眠的函数
2. **EOI 必需**：IRQ 处理程序必须发送 EOI
3. **IST 栈**：Double Fault 使用独立的 IST1 栈
4. **递归中断**：避免在中断处理程序中触发同样的中断

## 相关文档

- [idt - IDT](./idt.md)
- [apic - APIC](./apic.md)
- [interrupt - 中断子系统](./interrupt.md)
