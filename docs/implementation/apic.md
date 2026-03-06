# APIC

高级可编程中断控制器 (APIC) 包括 Local APIC 和 I/O APIC，用于多核系统中断管理和分发。

## 文件

- `kernel/src/interrupt/arch/x86_64/controller/apic.rs`

## Local APIC

### 初始化

```rust
pub unsafe fn init_local_apic(addr: u64)
```

**步骤**:
1. 映射 Local APIC 寄存器
2. 设置 DFR (Delivery Flat Model)
3. 设置 LVT (Local Vector Table)
4. 设置 TPR (Task Priority Register)
5. 启用 APIC

```rust
// 映射 Local APIC
let apic_base = VirtAddr::new(addr);

// 设置 DFR (Flat Model)
apic_base.write_u32(0xF0, 0xFFFFFFFF);

// 设置 spurious IRQ 向量
apic_base.write_u32(0xF0, 0xFF);

// 启用 APIC
let value = apic_base.read_u32(0xF0);
apic_base.write_u32(0xF0, value | 0x100);
```

### APIC Timer

**初始化**:
```rust
pub unsafe fn init_apic_timer(vector: u8, frequency_hz: u32)
```

**设置**:
```rust
// 设置 divider (16 = divide by 1)
apic_base.write_u32(0x3E0, 0x0003);

// 设置配置 (periodic mode)
let lvt_timer = apic_base.read_u32(0x320);
apic_base.write_u32(0x320, (lvt_timer & 0xFF00FF) | vector as u32);

// 设置初始计数值
let ticks = bus_frequency / frequency_hz;
apic_base.write_u32(0x380, ticks as u32);
```

**校准**:
```rust
pub fn calibrate_timer() -> u64
```

使用 PIT 测量总线频率，然后设置 APIC Timer。

### IPI (处理器间中断)

```rust
pub fn send_ipi(target_apic_id: u32, vector: u8, mode: IpiDeliveryMode)
pub fn send_ipi_all_excluding_self(vector: u8, mode: IpiDeliveryMode)
```

**发送 IPI**:
```rust
// 发送到指定 APIC
apic_base.write_u32(0x310, (target_apic_id << 24) as u32);
apic_base.write_u32(0x300, 0x4000 | (icr << 12) | vector as u32);

// 广播到所有 APIC（除自己）
apic_base.write_u32(0x300, 0xC000 | (icr << 12) | vector as u32);
```

### EOI (End of Interrupt)

```rust
pub fn local_apic_eoi()
```

```rust
// 写 EOI 寄存器
apic_base.write_u32(0xB0, 0);
```

## I/O APIC

### 初始化

```rust
pub unsafe fn init_ioapic(addr: u64, gsi_base: u32)
```

### IRQ 重映射

```rust
pub fn ioapic_set_irq(
    irq: u8,
    vector: u8,
    pin_polarity: u8,
    trigger_mode: bool,
    masked: bool
)
```

**Redirection Table Entry**:
```
Bit 0-7:    Interrupt Vector
Bit 8:      Delivery Mode (000 = Fixed)
Bit 9:      Destination Mode (0 = Physical)
Bit 10:    Reserved
Bit 11:    Delivery Status (只读)
Bit 12:    Polarity (0 = Active High, 1 = Active Low)
Bit 13:    Remote IRR (只读)
Bit 14:    Trigger Mode (0 = Edge, 1 = Level)
Bit 15:    Mask (0 = Enabled, 1 = Disabled)
Bit 16-63:  Destination
```

**示例**:
```rust
// 设置 IRQ 1 (键盘) 到向量 0x21
// 边沿触发、高电平有效、未屏蔽
ioapic_set_irq(1, 0x21, 0, false, false);
```

### 掩码/取消掩码

```rust
pub fn ioapic_mask_irq(irq: u8)
pub fn ioapic_unmask_irq(irq: u8)
```

## 寄存器

### Local APIC 寄存器

| 偏移 | 寄存器 | 说明 |
|------|--------|------|
| 0x020 | ID | APIC ID |
| 0x0B0 | EOI | End of Interrupt |
| 0x0D0 | LVT Timer | Timer Local Vector Table |
| 0x0F0 | LVT Thermal | Thermal LVT |
| 0x300 | ICR Low | Interrupt Command Register Low |
| 0x310 | ICR High | Interrupt Command Register High |
| 0x320 | LVT Timer | Timer LVT |
| 0x380 | Initial Count | Timer Initial Count |
| 0x390 | Current Count | Timer Current Count |
| 0x3E0 | Divide Config | Timer Divide Configuration |

### I/O APIC 寄存器

| 偏移 | 寄存器 | 说明 |
|------|--------|------|
| 0x00 | ID | I/O APIC ID |
| 0x01 | Version | Version |
| 0x10 | Redirection Table | IRQ 0 Redirection Table |
| 0x11 | Redirection Table | IRQ 1 Redirection Table |
| ... | ... | ... |

## 使用示例

### 初始化 APIC

```rust
use kernel::interrupt::{
    init_local_apic,
    init_ioapic,
    ioapic_set_irq,
    ioapic_unmask_irq,
    init_apic_timer,
    local_apic_id,
    calibrate_timer,
};

unsafe {
    // 初始化 Local APIC
    init_local_apic(0xFEE00000);

    // 初始化 I/O APIC
    init_ioapic(0xFEC00000, 0);

    // 设置键盘中断 (IRQ 1 -> vector 0x21)
    ioapic_set_irq(1, 0x21, 0, false, false);
    ioapic_unmask_irq(1);

    // 设置串口中断 (IRQ 4 -> vector 0x24)
    ioapic_set_irq(4, 0x24, 0, false, false);
    ioapic_unmask_irq(4);

    // 初始化定时器 (100 Hz)
    let bus_freq = calibrate_timer();
    init_apic_timer(0x20, 100);

    kprintln!("Local APIC ID: {}", local_apic_id());
}
```

### 发送 IPI

```rust
use kernel::interrupt::{send_ipi, send_ipi_all_excluding_self, IpiDeliveryMode};

// 发送到指定 CPU
send_ipi(1, 0x40, IpiDeliveryMode::Fixed);

// 广播到所有 CPU（除自己）
send_ipi_all_excluding_self(0x40, IpiDeliveryMode::Fixed);
```

### 定时器使用

```rust
// 获取定时器计数
let current_count = apic_base.read_u32(0x390);

// 读取总线频率
let bus_freq = apic_timer_frequency();
```

## 相关文档

- [API: apic](../api/interrupt/apic.md)
- [API: pit](../api/interrupt/pit.md)
- [GDT/TSS](./gdt.md)
- [IDT](./idt.md)
