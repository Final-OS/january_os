# pit - 可编程间隔定时器

PIT (8254 Programmable Interval Timer) 是传统的定时器，用于 APIC Timer 校准。

## API

### 频率设置

```rust
pub fn pit_set_frequency(hz: u32)
```

设置 PIT 频率。

**参数**：
- `hz`: 频率 (Hz)，范围约 19-1193182

**示例**：
```rust
use kernel::interrupt::pit_set_frequency;

// 设置 1000 Hz
pit_set_frequency(1000);
```

### 读取计数

```rust
pub fn pit_get_ticks() -> u16
pub fn pit_tick() -> bool
```

**示例**：
```rust
use kernel::interrupt::{pit_get_ticks, pit_tick};

// 读取当前计数值
let ticks = pit_get_ticks();

// 检查是否有 tick
if pit_tick() {
    kprintln!("Timer tick!");
}
```

## 常量

```rust
pub const PIT_FREQUENCY: u32 = 1193182; // 基准频率 (Hz)
```

## 寄存器

| 端口 | 访问 | 说明 |
|------|------|------|
| 0x40 | 写 | 通道 0 数据 |
| 0x41 | 写 | 通道 1 数据 |
| 0x42 | 写 | 通道 2 数据 |
| 0x43 | 写 | 控制字 |

## 控制字格式

```
7   6   5   4   3   2   1   0
┌───┬───┬───┬───┬───┬───┬───┬───┐
│SC │SC │RW │RW │M  │M  │M  │BIN│
│ 1 │ 0 │ 1 │ 0 │ 2 │ 1 │ 0 │   │
└───┴───┴───┴───┴───┴───┴───┴───┘

SC (Select Channel):
  00 = Channel 0
  01 = Channel 1
  10 = Channel 2
  11 = Read-back command

RW (Read/Write):
  00 = Latch count
  01 = Latch low byte only
  10 = Latch high byte only
  11 = Latch low then high byte

M (Mode):
  000 = Mode 0 (Interrupt on Terminal Count)
  001 = Mode 1 (Hardware Retriggerable One-Shot)
  010 = Mode 2 (Rate Generator)
  011 = Mode 3 (Square Wave Generator)
  100 = Mode 4 (Software Triggered Strobe)
  101 = Mode 5 (Hardware Triggered Strobe)

BIN: Binary (0) / BCD (1)
```

## 模式

### Mode 2: Rate Generator

周期性产生脉冲，用于时间基准。

```rust
// 设置 Mode 2, 通道 0, 低+高字节, 二进制
const CONTROL_WORD: u8 = 0b00_11_010_0;

unsafe {
    core::arch::asm!(
        "out dx, al",
        in("dx") 0x43u16,
        in("al") CONTROL_WORD,
    );
}
```

### Mode 3: Square Wave

生成方波，用于扬声器。

## 频率计算

```rust
// 计算分频值
let divisor = PIT_FREQUENCY / target_frequency;

// 发送低字节
let low = (divisor & 0xFF) as u8;
unsafe { core::arch::asm!("out dx, al", in("dx") 0x40u16, in("al") low); }

// 发送高字节
let high = ((divisor >> 8) & 0xFF) as u8;
unsafe { core::arch::asm!("out dx, al", in("dx") 0x40u16, in("al") high); }
```

## 使用示例

### 测量时间

```rust
use kernel::interrupt::{pit_set_frequency, pit_get_ticks};

// 设置高频率
pit_set_frequency(1000);

let start = pit_get_ticks();

// ... 做某事 ...

let elapsed = start.wrapping_sub(pit_get_ticks());
kprintln!("Elapsed: {} ticks", elapsed);
```

### 校准 APIC Timer

```rust
use kernel::interrupt::{
    pit_set_frequency,
    calibrate_timer,
};

// 设置 100 Hz
pit_set_frequency(100);

// 使用 PIT 校准 APIC Timer
let bus_freq = calibrate_timer();
kprintln!("Bus frequency: {} MHz", bus_freq / 1_000_000);
```

### 扬声器

```rust
fn speaker_on(freq: u32) {
    let divisor = (PIT_FREQUENCY / freq) as u16;

    // 设置通道 2
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x43u16,
            in("al") 0b10_11_011_0u8, // Channel 2, Mode 3
        );

        core::arch::asm!(
            "out dx, al",
            in("dx") 0x42u16,
            in("al") (divisor & 0xFF) as u8,
        );

        core::arch::asm!(
            "out dx, al",
            in("dx") 0x42u16,
            in("al") ((divisor >> 8) & 0xFF) as u8,
        );
    }

    // 启用扬声器
    let mut port: u8 = 0;
    unsafe {
        core::arch::asm!("in al, 0x61", out("al") port, options(nomem, nostack));
        port |= 0x03;
        core::arch::asm!("out 0x61, al", in("al") port, options(nostack));
    }
}

fn speaker_off() {
    let mut port: u8 = 0;
    unsafe {
        core::arch::asm!("in al, 0x61", out("al") port, options(nomem, nostack));
        port &= 0xFC;
        core::arch::asm!("out 0x61, al", in("al") port, options(nostack));
    }
}
```

## 注意事项

1. **仅用于校准**：PIT 主要用于校准 APIC Timer
2. **通道 0**：连接到 IRQ 0
3. **通道 1**：传统上用于内存刷新
4. **通道 2**：连接到 PC 扬声器

## 相关文档

- [apic - APIC](./apic.md)
- [实现: APIC](../../implementation/apic.md)
