# input - 输入设备驱动

输入子系统处理键盘和鼠标等输入设备。

## API

### PS/2 键盘

```rust
pub fn ps2_init()
pub fn read_char() -> Option<u8>
pub fn has_char() -> bool
pub fn buffer_len() -> usize
```

**初始化**：
```rust
use kernel::drivers::input::ps2_init;

ps2_init();
```

**读取字符**：
```rust
use kernel::drivers::input::{read_char, has_char};

while has_char() {
    if let Some(c) = read_char() {
        kprint!("{}", c as char);
    }
}
```

### 修饰键状态

```rust
pub fn last_scancode() -> u8
pub fn last_char() -> Option<u8>
pub fn is_shift_pressed() -> bool
pub fn is_ctrl_pressed() -> bool
pub fn is_alt_pressed() -> bool
```

**示例**：
```rust
use kernel::drivers::input::{
    is_shift_pressed, is_ctrl_pressed, is_alt_pressed,
};

if is_ctrl_pressed() && is_alt_pressed() {
    kprintln!("Ctrl+Alt pressed");
}

if is_shift_pressed() {
    kprintln!("Shift pressed");
}
```

### USB HID (Boot Protocol)

内核实现了基本的 USB HID Boot Protocol 支持，可以无缝处理 USB 键盘和鼠标输入。

#### API

```rust
// 初始化 USB 鼠标/键盘驱动
pub fn usb_mouse_init()
pub fn usb_keyboard_init()

// 处理 Boot Protocol 报告 (供 xHCI 驱动调用)
pub fn handle_boot_report(report: BootReport)
```

#### 事件处理

USB 键盘事件通过 `KeyEvent` 结构体传递，并自动整合到系统的字符输入流中。

```rust
pub struct KeyEvent {
    pub scancode: u8,
    pub pressed: bool,
    pub modifiers: Modifiers,
    pub ascii: Option<u8>,
}
```

USB 鼠标事件通过 `MouseEvent` 结构体传递：

```rust
pub struct MouseEvent {
    pub event_type: MouseEventType, // Move, ButtonDown, ButtonUp, Scroll
    pub dx: i32,
    pub dy: i32,
    pub scroll: i32,
    pub button: Option<MouseButton>,
    pub buttons: u8,
}
```

#### 缓冲区管理

为了避免高频事件阻塞输入，USB 输入驱动采用了事件缓冲区与字符缓冲区解耦的设计：
1. **事件缓冲区**：存储原始按键/鼠标事件，用于低级处理。
2. **字符缓冲区**：存储解码后的 ASCII 字符，供 Shell 等上层应用消费。

即使事件缓冲区已满（例如未被消费），字符输入仍能正常写入字符缓冲区，确保 Shell 交互的流畅性。

## PS/2 端口

### 端口地址

| 端口 | 地址 | 说明 |
|------|------|------|
| 数据 | 0x60 | 读写数据 |
| 状态 | 0x64 | 读取状态 |
| 命令 | 0x64 | 发送命令 |

### 状态寄存器

```
7   6   5   4   3   2   1   0
┌───┬───┬───┬───┬───┬───┬───┬───┐
│out│in │aux│timeout│ │ │ │ │
└───┴───┴───┴───┴───┴───┴───┴───┘

out: 输出缓冲区满 (1 = 可读)
in: 输入缓冲区满 (1 = 不能写)
aux: 鼠标数据可用
```

### 常用命令

| 命令 | 说明 |
|------|------|
| 0xED | 设置 LED |
| 0xEE | 回送 (Echo) |
| 0xF0 | 设置/获取扫描码集 |
| 0xF2 | 读取键盘 ID |
| 0xF3 | 设置 typematic rate |
| 0xF4 | 启用键盘 |
| 0xF5 | 禁用键盘 |
| 0xFF | 复位 |

## 扫描码

### Set 1 扫描码 (默认)

| 键 | 按下 | 释放 |
|----|------|------|
| A | 0x1E | 0x9E |
| B | 0x30 | 0xB0 |
| Enter | 0x1C | 0x9C |
| Space | 0x39 | 0xB9 |
| Esc | 0x01 | 0x81 |
| Backspace | 0x0E | 0x8E |

### 扩展扫描码

以 `0xE0` 开头：

| 组合 | 扫描码 |
|------|--------|
| Print Screen | E0 2A E0 37 |
| Pause | E1 1D 45 E1 9D C5 |
| Left Ctrl | 0x1D |
| Right Ctrl | E0 1D |
| Left Alt | 0x38 |
| Right Alt | E0 38 |

## USB HID

### HID 报告描述符

```rust
pub struct HidReportDescriptor {
    pub data: &[u8],
}

impl HidReportDescriptor {
    pub fn parse(&self) -> Result<HidInfo, HidError>
}
```

### HID 设备类型

```rust
pub enum HidType {
    Keyboard,
    Mouse,
    Other,
}
```

### 键盘报告

```
Byte 0: 修饰键 (Ctrl, Shift, Alt, GUI)
  Bit 0: Left Ctrl
  Bit 1: Left Shift
  Bit 2: Left Alt
  Bit 3: Left GUI
  Bit 4: Right Ctrl
  Bit 5: Right Shift
  Bit 6: Right Alt
  Bit 7: Right GUI

Byte 1: 保留 (0)

Byte 2-7: 按下键 (HID Usage IDs)
```

### 鼠标报告

```
Byte 0: 按钮
  Bit 0: 左键
  Bit 1: 右键
  Bit 2: 中键

Byte 1: X 偏移 (有符号)

Byte 2: Y 偏移 (有符号)
```

## 使用示例

### 键盘输入处理

```rust
use kernel::drivers::input::{read_char, has_char, is_ctrl_pressed};

fn process_input() {
    let mut buffer = [0u8; 64];
    let mut len = 0;

    loop {
        while has_char() {
            if let Some(c) = read_char() {
                match c {
                    8 | 127 => { // Backspace/Delete
                        if len > 0 {
                            len -= 1;
                            kprint!("\x08 \x08");
                        }
                    }
                    b'\n' | b'\r' => {
                        kprintln!();
                        execute_command(&buffer[..len]);
                        len = 0;
                        kprint!("> ");
                    }
                    3 => { // Ctrl+C
                        if is_ctrl_pressed() {
                            kprintln!("^C");
                            len = 0;
                            kprint!("> ");
                        }
                    }
                    c if c >= 32 && c < 127 => {
                        if len < 63 {
                            buffer[len] = c;
                            len += 1;
                            kprint!("{}", c as char);
                        }
                    }
                    _ => {}
                }
            }
        }
        halt_with_interrupts();
    }
}
```

### USB HID 键盘

```rust
use kernel::drivers::input::hid::handle_hid_report;

// HID 键盘报告 (8 字节)
let report: [u8; 8] = [
    0x01,  // Left Ctrl
    0x00,  // Reserved
    0x04,  // Key 'a' (HID Usage 0x04)
    0x00, 0x00, 0x00, 0x00, 0x00,  // No other keys
];

handle_hid_report(&report);
```

### USB HID 鼠标

```rust
use kernel::drivers::input::hid::handle_mouse_report;

// HID 鼠标报告 (3 字节)
let report: [u8; 3] = [
    0x01,  // 左键按下
    0x10,  // X = +16
    0x08,  // Y = +8
];

handle_mouse_report(&report);
```

## 输入缓冲区

```rust
// 环形缓冲区
struct InputBuffer {
    data: [u8; 256],
    head: usize,
    tail: usize,
}

impl InputBuffer {
    fn push(&mut self, c: u8) -> bool {
        // 添加字符
    }

    fn pop(&mut self) -> Option<u8> {
        // 取出字符
    }

    fn len(&self) -> usize {
        // 缓冲区长度
    }

    fn is_empty(&self) -> bool {
        // 是否为空
    }
}
```

## 扫描码到字符转换

```rust
fn scancode_to_char(scancode: u8, shift: bool) -> Option<char> {
    // 美国布局
    let map = if shift {
        &SHIFT_KEYMAP
    } else {
        &KEYMAP
    };

    map.get(&scancode).copied()
}

const KEYMAP: &[u8] = &[
    0, 0, '1' as u8, '2' as u8, '3' as u8, '4' as u8,
    '5' as u8, '6' as u8, '7' as u8, '8' as u8, '9' as u8,
    '0' as u8, '-', '=', '\t' as u8,
    'q' as u8, 'w' as u8, 'e' as u8, 'r' as u8,
    // ...
];
```

## 相关文档

- [tty - TTY 子系统](./tty.md)
- [interrupt - 中断处理](../interrupt/interrupt.md)
