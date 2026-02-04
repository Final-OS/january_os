# tty - TTY 子系统

TTY 子系统管理终端设备，包括串口、控制台和伪终端。

## API

### 初始化

```rust
pub fn serial_init()
pub fn serial_enable_rx_interrupt()
pub fn serial_try_read() -> Option<u8>
```

**串口初始化**：
```rust
use kernel::drivers::tty::{serial_init, serial_enable_rx_interrupt};

serial_init();
serial_enable_rx_interrupt();
```

**非阻塞读取**：
```rust
use kernel::drivers::tty::serial_try_read;

while let Some(c) = serial_try_read() {
    kprintln!("Received: {}", c as char);
}
```

### 写入

```rust
use core::fmt::Write;
use kernel::drivers::tty::{SerialWriter, FbConsoleWriter};

// 串口写入
write!(SerialWriter, "Hello, serial!\n").ok();

// Framebuffer 写入
write!(FbConsoleWriter, "Hello, console!\n").ok();
```

### 控制台

```rust
pub fn console_init(
    width: u32,
    height: u32,
    stride: u32,
    pixel_format: u32
)

pub fn switch_to_console(n: usize)
pub fn get_current_console() -> usize
```

**初始化控制台**：
```rust
use kernel::drivers::tty::console_init;

console_init(1024, 768, 1024, 1); // BGR 格式
```

**切换虚拟终端**：
```rust
use kernel::drivers::tty::switch_to_console;

switch_to_console(1); // 切换到 tty1
switch_to_console(2); // 切换到 tty2
```

## 串口配置

### 端口地址

| 端口 | 基地址 | IRQ |
|------|--------|-----|
| COM1 | 0x3F8 | 4 |
| COM2 | 0x2F8 | 3 |
| COM3 | 0x3E8 | 4 |
| COM4 | 0x2E8 | 3 |

### 波特率

```rust
const DEFAULT_BAUD: u32 = 38400;
```

常用波特率：
- 9600
- 19200
- 38400 (默认)
- 57600
- 115200

### 寄存器

| 偏移 | DLAB | 读 | 写 | 说明 |
|------|------|--- |--- |------|
| +0 | 0 | RX | TX | 数据 |
| +0 | 1 | - | DLL | 除数低字节 |
| +1 | 0 | IER | - | 中断使能 |
| +1 | 1 | - | DLH | 除数高字节 |
| +2 | - | FCR | IIR | FIFO 控制/中断 ID |
| +3 | - | LCR | - | 线路控制 |
| +4 | - | MCR | - | 调制解调器控制 |
| +5 | - | LSR | - | 线路状态 |
| +6 | - | MSR | - | 调制解调器状态 |
| +7 | - | SCR | - | 暂存寄存器 |

## 控制台 VT100 支持

### 转义序列

| 序列 | 功能 |
|------|------|
| `\x1b[2J` | 清屏 |
| `\x1b[H` | 光标移到左上角 |
| `\x1b[n;mH` | 光标移到 n 行 m 列 |
| `\x1b[nA` | 光标上移 n 行 |
| `\x1b[nB` | 光标下移 n 行 |
| `\x1b[nC` | 光标右移 n 列 |
| `\x1b[nD` | 光标左移 n 列 |
| `\x1b[?25l` | 隐藏光标 |
| `\x1b[?25h` | 显示光标 |
| `\x1b[30m` | 黑色文本 |
| `\x1b[31m` | 红色文本 |
| `\x1b[32m` | 绿色文本 |
| `\x1b[33m` | 黄色文本 |
| `\x1b[34m` | 蓝色文本 |
| `\x1b[35m` | 品红文本 |
| `\x1b[36m` | 青色文本 |
| `\x1b[37m` | 白色文本 |
| `\x1b[0m` | 重置属性 |
| `\x1b[1m` | 粗体 |
| `\x1b[4m` | 下划线 |
| `\x1b[7m` | 反色 |

### 使用示例

```rust
// 清屏并设置颜色
console_write("\x1b[2J");           // 清屏
console_write("\x1b[H");            // 光标移到左上角
console_write("\x1b[31m");          // 红色文本
console_write("Error message\n");
console_write("\x1b[0m");           // 重置
```

## 伪终端

```rust
pub fn ptmx_open() -> Option<PTYMaster>
pub fn pts_open(index: usize) -> Option<PTYSlave>
```

**示例**：
```rust
use kernel::drivers::tty::{ptmx_open, pts_open};

// 打开主设备
let master = ptmx_open()?;

// 打开从设备
let slave = pts_open(master.index())?;

// 读写
master.write(b"Hello from master");
let data = slave.read(32);
```

## 使用示例

### 串口输出

```rust
use core::fmt::Write;
use kernel::drivers::tty::SerialWriter;

// 简单输出
write!(SerialWriter, "Hello, World!\n").ok();

// 格式化输出
let value = 42;
write!(SerialWriter, "Value: {}\n", value).ok();

// 十六进制
write!(SerialWriter, "Address: {:#x}\n", 0xdeadbeef).ok();
```

### Framebuffer 输出

```rust
use kernel::drivers::tty::{FbConsoleWriter, switch_to_console};

// 切换到 tty2
switch_to_console(2);

// 输出
write!(FbConsoleWriter, "TTY2 Output\n").ok();

// VT100 颜色
write!(FbConsoleWriter, "\x1b[31mRed text\x1b[0m\n").ok();
```

### 读取输入

```rust
use kernel::drivers::tty::serial_try_read;
use kernel::interrupt::{read_char, has_char};

// 非阻塞读取
if let Some(c) = serial_try_read() {
    kprintln!("Received: {}", c as char);
}

// 从输入缓冲区读取（键盘+串口）
while has_char() {
    if let Some(c) = read_char() {
        match c {
            b'\n' => kprintln!("Enter"),
            c => kprint!("{}", c as char),
        }
    }
}
```

## TTY 设备编号

| 设备 | 名称 | 说明 |
|------|------|------|
| tty1-tty6 | 虚拟终端 | Framebuffer 控制台 |
| ttyS0-ttyS3 | 串口 | COM1-COM4 |
| ptmx | 伪终端主设备 | 主/从对 |
| pts/0-pts/255 | 伪终端从设备 | 从设备 |

## 注意事项

1. **VT100 解析**：转义序列解析是基础的，不是完整的 VT100
2. **串口中断**：启用中断后需要发送 EOI
3. **缓冲区大小**：输入缓冲区有限，需要及时读取
4. **并发**：多个写入者可能导致输出混乱

## 相关文档

- [实现: TTY 子系统](../../implementation/tty.md)
