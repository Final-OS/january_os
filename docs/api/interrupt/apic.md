# apic - APIC 中断控制器

APIC (Advanced Programmable Interrupt Controller) 包括 Local APIC 和 I/O APIC。

## API

### Local APIC

```rust
pub unsafe fn init_local_apic(addr: u64)
pub fn local_apic_id() -> u32
pub fn local_apic_eoi()
```

**初始化 Local APIC**：
```rust
use kernel::interrupt::init_local_apic;

unsafe {
    init_local_apic(0xFEE00000);
}
```

**获取 APIC ID**：
```rust
use kernel::interrupt::local_apic_id;

let id = local_apic_id();
kprintln!("APIC ID: {}", id);
```

**发送 EOI**：
```rust
use kernel::interrupt::local_apic_eoi;

// 中断处理程序结束
local_apic_eoi();
```

### I/O APIC

```rust
pub unsafe fn init_ioapic(addr: u64, gsi_base: u32)
pub fn ioapic_set_irq(
    irq: u8,
    vector: u8,
    pin_polarity: u8,
    trigger_mode: bool,
    masked: bool
)
pub fn ioapic_mask_irq(irq: u8)
pub fn ioapic_unmask_irq(irq: u8)
```

**初始化 I/O APIC**：
```rust
use kernel::interrupt::init_ioapic;

unsafe {
    init_ioapic(0xFEC00000, 0);
}
```

**设置 IRQ**：
```rust
use kernel::interrupt::{ioapic_set_irq, ioapic_mask_irq, ioapic_unmask_irq};

// 设置 IRQ 1 (键盘) 到向量 0x21
ioapic_set_irq(1, 0x21, 0, false, false);

// 启用中断
ioapic_unmask_irq(1);
```

### APIC Timer

```rust
pub unsafe fn init_apic_timer(vector: u8, frequency_hz: u32)
pub fn stop_apic_timer()
pub fn calibrate_timer() -> u64
pub fn apic_timer_frequency() -> u64
```

**初始化定时器**：
```rust
use kernel::interrupt::{init_apic_timer, calibrate_timer};

// 校准总线频率
let bus_freq = calibrate_timer();
kprintln!("Bus frequency: {} MHz", bus_freq / 1_000_000);

// 初始化 100 Hz 定时器
unsafe {
    init_apic_timer(0x20, 100);
}
```

**停止定时器**：
```rust
use kernel::interrupt::stop_apic_timer;

stop_apic_timer();
```

### IPI (处理器间中断)

```rust
pub enum IpiDeliveryMode {
    Fixed = 0,
    LowestPriority = 1,
    SMI = 2,
    NMI = 4,
    Init = 5,
    Startup = 6,
}

pub fn send_ipi(target_apic_id: u32, vector: u8, mode: IpiDeliveryMode)
pub fn send_ipi_all_excluding_self(vector: u8, mode: IpiDeliveryMode)
```

**发送 IPI**：
```rust
use kernel::interrupt::{send_ipi, send_ipi_all_excluding_self, IpiDeliveryMode};

// 发送到指定 CPU
send_ipi(1, 0x40, IpiDeliveryMode::Fixed);

// 广播到所有 CPU（除自己）
send_ipi_all_excluding_self(0x40, IpiDeliveryMode::Fixed);
```

## 寄存器

### Local APIC 寄存器

| 偏移 | 寄存器 | 说明 |
|------|--------|------|
| 0x020 | ID | APIC ID |
| 0x0B0 | EOI | End of Interrupt |
| 0x0D0 | LVT Timer | Timer 局部向量表 |
| 0x0E0 | LVT Thermal | Thermal 监控 |
| 0x0F0 | LVT Performance | 性能监控 |
| 0x100-0x170 | ICR | IPI 命令 |
| 0x280 | TPR | 任务优先级 |
| 0x320 | ESR | 错误状态 |
| 0x380 | ICR | 中断命令 |
| 0x3E0 | Timer LVT | 定时器配置 |
| 0x380 | Initial Count | 定时器初值 |
| 0x390 | Current Count | 定时器当前值 |
| 0x3E0 | Divide Config | 定时器分频 |

### I/O APIC 寄存器

| 偏移 | 寄存器 | 说明 |
|------|--------|------|
| 0x00 | ID | I/O APIC ID |
| 0x01 | Version | 版本 |
| 0x10 | Redirection Table | 重映射表 (IRQ 0) |
| 0x11 | Redirection Table | 重映射表 (IRQ 1) |
| ... | ... | ... |

## 定时器模式

```rust
pub enum TimerMode {
    OneShot = 0,      // 单次触发
    Periodic = 1,     // 周期性
    TSCDeadline = 2,  // TSC 截止 (MSR 方式)
}
```

## I/O APIC 重映射表

```
Redirection Table Entry (64 位):

63            56 55   48 47             32
├───────────────┼──────┼────────────────┤
│ Reserved      │ Mask │ Destination    │
├───────────────┴──────┴────────────────┤
│  │  │  │  │  │  │  │  │  Trigger │ TM│
│  │  │  │  │  │  │  │  └─ Mode     │  │
│  │  │  │  │  │  │  └──── Remote IRR│  │
│  │  │  │  │  │  └─────── Polarity │  │
│  │  │  │  │  └──────────── Delivery Mode│
│  │  │  │  └───────────────│        │  │
│  │  │  └──────────────────┴────────│  │
│  │  └─────────────────────────────┼────┤
│  └────────────────────────────────┴────┤
│            Vector (8 bits)           │
└───────────────────────────────────────┘

Vector: 中断向量 (0-255)
Delivery Mode:
  000 = Fixed
  001 = Lowest Priority
  010 = SMI
  100 = NMI
  101 = Init
  110 = Startup
Destination Mode:
  0 = Physical
  1 = Logical
Delivery Status: (只读)
  0 = Idle
  1 = Pending
Polarity:
  0 = Active High
  1 = Active Low
Trigger Mode:
  0 = Edge
  1 = Level
Mask:
  0 = Not Masked (启用)
  1 = Masked (禁用)
```

## 使用示例

### 设置键盘中断

```rust
use kernel::interrupt::{ioapic_set_irq, ioapic_unmask_irq, IRQ_KEYBOARD};

// 键盘: IRQ 1 -> Vector 0x21
// 边沿触发，高电平有效
ioapic_set_irq(1, IRQ_KEYBOARD, 0, false, false);
ioapic_unmask_irq(1);
```

### 设置串口中断

```rust
use kernel::interrupt::{ioapic_set_irq, ioapic_unmask_irq, IRQ_COM1};

// COM1: IRQ 4 -> Vector 0x24
ioapic_set_irq(4, IRQ_COM1, 0, false, false);
ioapic_unmask_irq(4);
```

## 相关文档

- [gdt - GDT](./gdt.md)
- [idt - IDT](./idt.md)
- [pit - PIT](./pit.md)
- [实现: APIC](../../implementation/apic.md)
