# TTY 子系统

TTY 子系统管理终端设备，包括串口、控制台和伪终端。

## 文件

- `kernel/src/drivers/tty/mod.rs` - TTY 主模块
- `kernel/src/drivers/tty/serial/mod.rs` - 串口驱动
- `kernel/src/drivers/tty/console/mod.rs` - Framebuffer 控制台
- `kernel/src/drivers/tty/pty/mod.rs` - 伪终端

## TTY 架构

```
用户空间
    │ open, read, write, ioctl
    ▼
┌─────────────────────────────────────────────────────────┐
│                     TTY 核心层                          │
│  ┌─────────────────────────────────────────────────────┐│
│  │          线路规范 (termios, VT100)               ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│                    TTY 驱动                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐             │
│  │  Serial  │ │ Console  │ │    PTY       │             │
│  │  TTY     │ │  (tty1-6)│ │  (ptmx/pts)  │             │
│  └──────────┘ └──────────┘ └──────────────┘             │
└─────────────────────────────────────────────────────────┘
```

## 串口 TTY

### 初始化

```rust
pub fn serial_init()
```

**步骤**:
1. 检测端口 (COM1-COM4)
2. 配置波特率
3. 配置线路控制 (8N1)
4. 启用 FIFO
5. 启用中断

```rust
// 配置 16550 UART
unsafe {
    // 配置波特率 (DLH:DLL = 115200 / (16 * divisor))
    outb(0x3FB, 0x80);  // 设置 DLAB
    outb(0x3F8, 0x01);   // DLL
    outb(0x3F9, 0x00);   // DLH
    outb(0x3FB, 0x03);   // 8-bit, no parity

    // 启用 FIFO
    outb(0x3FA, 0x01);  // FIFO enable
    outb(0x3F9, 0x07);   // clear FIFO, 14-byte threshold

    // 启用中断
    outb(0x3F9, 0x0F);   // enable interrupt
}
```

### 中断处理

```rust
extern "x86-interrupt" fn serial_handler(_frame: InterruptFrame) {
    // 读取字符
    let c = read_from_com1();

    // 添加到输入缓冲区
    SERIAL_INPUT_BUFFER.lock().push(c);

    // 发送 EOI
    local_apic_eoi();
}
```

### 非阻塞读取

```rust
pub fn serial_try_read() -> Option<u8>
```

## Framebuffer 控制台

### 初始化

```rust
pub fn console_init(
    width: u32,
    height: u32,
    stride: u32,
    pixel_format: u32
)
```

**Console 状态**:
```rust
pub struct Console {
    pub width: u32,
    pub height: u32,
    pub fb: VirtAddr,          // Framebuffer 地址
    pub font: &'static PsfFont,
    pub buffer: [[CharCell; WIDTH]; HEIGHT],
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub cursor_visible: bool,
}
```

### VT100 解析

```rust
pub fn parse_escape_sequence(seq: &[u8])
```

**支持的转义序列**:
- `\x1b[2J` - 清屏
- `\x1b[H` - 光标到左上角
- `\x1b[n;mH` - 光标到 n 行 m 列
- `\x1b[31m` - 红色文本
- `\x1b[0m` - 重置属性

**示例**:
```rust
console_write("\x1b[31m");  // 红色
console_write("Error: ");
console_write("\x1b[0m");   // 重置
console_write("message\n");
```

### 虚拟终端切换

```rust
pub fn switch_to_console(n: usize)
pub fn get_current_console() -> usize
```

**6 个虚拟终端**:
```rust
static CONSOLES: [Mutex<Option<Console>>; 6] = [
    Mutex::new(None), Mutex::new(None),
    Mutex::new(None), Mutex::new(None),
    Mutex::new(None), Mutex::new(None),
];
```

## 伪终端 (PTY)

### 主从设备

```rust
pub struct PtyMaster {
    pub index: usize,
    pub slave: Arc<Mutex<PtySlave>>,
}

pub struct PtySlave {
    pub index: usize,
    pub master: Weak<Mutex<PtyMaster>>,
    pub buffer: Vec<u8>,
}
```

### 创建 PTY

```rust
pub fn ptmx_open() -> Option<PtyMaster>
```

```rust
// 分配新的 PTY 对
let index = allocate_pty_index()?;

let (master, slave) = Pty::new_pair(index);
PTY_MASTERS.lock()[index] = Some(master);
PTY_SLAVES.lock()[index] = Some(slave);

Some(master)
```

## 使用示例

### 串口输出

```rust
use core::fmt::Write;
use kernel::drivers::tty::SerialWriter;

write!(SerialWriter, "Hello, serial!\n").ok();
```

### 控制台输出

```rust
use kernel::drivers::tty::{console_init, FbConsoleWriter};

console_init(1024, 768, 1024, 1);

write!(FbConsoleWriter, "Hello, console!\n").ok();
```

### 切换虚拟终端

```rust
use kernel::drivers::tty::switch_to_console;

// 切换到 tty2
switch_to_console(2);

// 输出会到 tty2
write!(FbConsoleWriter, "TTY2 Output\n").ok();
```

### VT100 颜色输出

```rust
// 红色错误信息
write!(FbConsoleWriter, "\x1b[31mError: {}\x1b[0m\n", error).ok();

// 绿色成功信息
write!(FbConsoleWriter, "\x1b[32mSuccess!\x1b[0m\n").ok();
```

## 相关文档

- [API: tty](../api/drivers/tty.md)
- [输入驱动](../api/drivers/input.md)
