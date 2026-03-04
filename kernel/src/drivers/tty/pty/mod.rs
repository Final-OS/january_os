//! 伪终端 (PTY) 实现
//!
//! PTY 提供一对虚拟终端设备：
//! - Master (ptmx): 控制端，连接到终端模拟器
//! - Slave (pts/N): 从端，连接到应用程序
//!
//! # 架构
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    用户空间                                  │
//! │  ┌─────────────┐              ┌─────────────┐               │
//! │  │ 终端模拟器  │              │    Shell    │               │
//! │  │ (xterm等)   │              │   (bash)    │               │
//! │  └──────┬──────┘              └──────┬──────┘               │
//! │         │                            │                      │
//! └─────────┼────────────────────────────┼──────────────────────┘
//!           │                            │
//! ┌─────────┼────────────────────────────┼──────────────────────┐
//! │         │          内核空间          │                      │
//! │         ▼                            ▼                      │
//! │  ┌─────────────┐    数据流    ┌─────────────┐               │
//! │  │   Master    │◄────────────►│    Slave    │               │
//! │  │  (/dev/ptmx)│              │ (/dev/pts/N)│               │
//! │  └─────────────┘              └─────────────┘               │
//! │         │                            │                      │
//! │         └──────────┬─────────────────┘                      │
//! │                    ▼                                        │
//! │            ┌─────────────┐                                  │
//! │            │ Line Disc.  │ (行规程)                         │
//! │            └─────────────┘                                  │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use crate::sync::Once;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

// ============================================================================
// 常量
// ============================================================================

/// 最大 PTY 数量
pub const MAX_PTYS: usize = 256;

/// PTY 缓冲区大小
pub const PTY_BUFFER_SIZE: usize = 4096;

/// PTY 主设备号
pub const PTY_MASTER_MAJOR: u32 = 2;

/// PTY 从设备号 (pts)
pub const PTY_SLAVE_MAJOR: u32 = 136;

// ============================================================================
// 终端属性 (termios)
// ============================================================================

/// 输入模式标志
#[derive(Debug, Clone, Copy)]
pub struct InputFlags(pub u32);

impl InputFlags {
    /// 忽略 BREAK 条件
    pub const IGNBRK: u32 = 0x0001;
    /// BREAK 产生中断
    pub const BRKINT: u32 = 0x0002;
    /// 忽略带奇偶校验错误的字符
    pub const IGNPAR: u32 = 0x0004;
    /// 奇偶校验错误标记
    pub const PARMRK: u32 = 0x0008;
    /// 启用输入奇偶校验
    pub const INPCK: u32 = 0x0010;
    /// 去除第 8 位
    pub const ISTRIP: u32 = 0x0020;
    /// NL 转 CR
    pub const INLCR: u32 = 0x0040;
    /// 忽略 CR
    pub const IGNCR: u32 = 0x0080;
    /// CR 转 NL
    pub const ICRNL: u32 = 0x0100;
    /// 大写转小写
    pub const IUCLC: u32 = 0x0200;
    /// 启用 XON/XOFF 流控
    pub const IXON: u32 = 0x0400;
    /// 任意字符重启输出
    pub const IXANY: u32 = 0x0800;
    /// 启用输入流控
    pub const IXOFF: u32 = 0x1000;
    /// 输入队列满时响铃
    pub const IMAXBEL: u32 = 0x2000;
    /// UTF-8 输入
    pub const IUTF8: u32 = 0x4000;
}

/// 输出模式标志
#[derive(Debug, Clone, Copy)]
pub struct OutputFlags(pub u32);

impl OutputFlags {
    /// 启用输出处理
    pub const OPOST: u32 = 0x0001;
    /// 小写转大写
    pub const OLCUC: u32 = 0x0002;
    /// NL 转 CR-NL
    pub const ONLCR: u32 = 0x0004;
    /// CR 转 NL
    pub const OCRNL: u32 = 0x0008;
    /// 列 0 不输出 CR
    pub const ONOCR: u32 = 0x0010;
    /// NL 执行 CR 功能
    pub const ONLRET: u32 = 0x0020;
}

/// 本地模式标志
#[derive(Debug, Clone, Copy)]
pub struct LocalFlags(pub u32);

impl LocalFlags {
    /// 产生信号
    pub const ISIG: u32 = 0x0001;
    /// 规范模式 (行编辑)
    pub const ICANON: u32 = 0x0002;
    /// 回显输入字符
    pub const ECHO: u32 = 0x0008;
    /// 回显擦除字符
    pub const ECHOE: u32 = 0x0010;
    /// 回显 KILL 字符
    pub const ECHOK: u32 = 0x0020;
    /// 回显 NL
    pub const ECHONL: u32 = 0x0040;
    /// 禁用刷新
    pub const NOFLSH: u32 = 0x0080;
    /// 发送 SIGTTOU
    pub const TOSTOP: u32 = 0x0100;
    /// 回显控制字符
    pub const ECHOCTL: u32 = 0x0200;
    /// 启用扩展处理
    pub const IEXTEN: u32 = 0x8000;
}

/// 控制字符索引
#[derive(Debug, Clone, Copy)]
#[repr(usize)]
pub enum ControlChar {
    VINTR = 0,  // ^C
    VQUIT = 1,  // ^\
    VERASE = 2, // ^?
    VKILL = 3,  // ^U
    VEOF = 4,   // ^D
    VTIME = 5,
    VMIN = 6,
    VSWTC = 7,
    VSTART = 8, // ^Q
    VSTOP = 9,  // ^S
    VSUSP = 10, // ^Z
    VEOL = 11,
    VREPRINT = 12,
    VDISCARD = 13,
    VWERASE = 14,
    VLNEXT = 15,
    VEOL2 = 16,
}

/// 控制字符数量
pub const NCCS: usize = 32;

/// 终端属性 (termios)
#[derive(Debug, Clone, Copy)]
pub struct Termios {
    /// 输入模式
    pub c_iflag: u32,
    /// 输出模式
    pub c_oflag: u32,
    /// 控制模式
    pub c_cflag: u32,
    /// 本地模式
    pub c_lflag: u32,
    /// 行规程
    pub c_line: u8,
    /// 控制字符
    pub c_cc: [u8; NCCS],
    /// 输入波特率
    pub c_ispeed: u32,
    /// 输出波特率
    pub c_ospeed: u32,
}

impl Default for Termios {
    fn default() -> Self {
        let mut c_cc = [0u8; NCCS];
        c_cc[ControlChar::VINTR as usize] = 0x03; // ^C
        c_cc[ControlChar::VQUIT as usize] = 0x1C; // ^\
        c_cc[ControlChar::VERASE as usize] = 0x7F; // DEL
        c_cc[ControlChar::VKILL as usize] = 0x15; // ^U
        c_cc[ControlChar::VEOF as usize] = 0x04; // ^D
        c_cc[ControlChar::VSTART as usize] = 0x11; // ^Q
        c_cc[ControlChar::VSTOP as usize] = 0x13; // ^S
        c_cc[ControlChar::VSUSP as usize] = 0x1A; // ^Z
        c_cc[ControlChar::VMIN as usize] = 1;

        Self {
            c_iflag: InputFlags::ICRNL | InputFlags::IXON,
            c_oflag: OutputFlags::OPOST | OutputFlags::ONLCR,
            c_cflag: 0o0060, // CS8
            c_lflag: LocalFlags::ISIG
                | LocalFlags::ICANON
                | LocalFlags::ECHO
                | LocalFlags::ECHOE
                | LocalFlags::ECHOK
                | LocalFlags::ECHOCTL
                | LocalFlags::IEXTEN,
            c_line: 0,
            c_cc,
            c_ispeed: 38400,
            c_ospeed: 38400,
        }
    }
}

// ============================================================================
// 窗口大小
// ============================================================================

/// 窗口大小
#[derive(Debug, Clone, Copy, Default)]
pub struct WinSize {
    /// 行数
    pub ws_row: u16,
    /// 列数
    pub ws_col: u16,
    /// 像素宽度
    pub ws_xpixel: u16,
    /// 像素高度
    pub ws_ypixel: u16,
}

// ============================================================================
// PTY 缓冲区
// ============================================================================

/// 环形缓冲区
pub struct RingBuffer {
    /// 数据
    data: [u8; PTY_BUFFER_SIZE],
    /// 读指针
    read_pos: usize,
    /// 写指针
    write_pos: usize,
    /// 数据量
    count: usize,
}

impl RingBuffer {
    /// 创建空缓冲区
    pub const fn new() -> Self {
        Self {
            data: [0; PTY_BUFFER_SIZE],
            read_pos: 0,
            write_pos: 0,
            count: 0,
        }
    }

    /// 写入数据
    pub fn write(&mut self, data: &[u8]) -> usize {
        let mut written = 0;
        for &byte in data {
            if self.count >= PTY_BUFFER_SIZE {
                break;
            }
            self.data[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % PTY_BUFFER_SIZE;
            self.count += 1;
            written += 1;
        }
        written
    }

    /// 读取数据
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut read = 0;
        for slot in buf.iter_mut() {
            if self.count == 0 {
                break;
            }
            *slot = self.data[self.read_pos];
            self.read_pos = (self.read_pos + 1) % PTY_BUFFER_SIZE;
            self.count -= 1;
            read += 1;
        }
        read
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.count >= PTY_BUFFER_SIZE
    }

    /// 可用空间
    pub fn available(&self) -> usize {
        PTY_BUFFER_SIZE - self.count
    }

    /// 数据量
    pub fn len(&self) -> usize {
        self.count
    }

    /// 清空
    pub fn clear(&mut self) {
        self.read_pos = 0;
        self.write_pos = 0;
        self.count = 0;
    }
}

// ============================================================================
// PTY 对
// ============================================================================

/// PTY 对状态
pub struct PtyPair {
    /// PTY 编号
    pub index: u32,
    /// 是否已分配
    pub allocated: bool,
    /// 是否打开
    pub master_open: bool,
    pub slave_open: bool,
    /// Master -> Slave 缓冲区
    pub to_slave: RingBuffer,
    /// Slave -> Master 缓冲区
    pub to_master: RingBuffer,
    /// 终端属性
    pub termios: Termios,
    /// 窗口大小
    pub winsize: WinSize,
    /// 行缓冲区 (规范模式)
    pub line_buffer: [u8; 256],
    pub line_len: usize,
}

impl PtyPair {
    /// 创建新的 PTY 对
    pub const fn new(index: u32) -> Self {
        Self {
            index,
            allocated: false,
            master_open: false,
            slave_open: false,
            to_slave: RingBuffer::new(),
            to_master: RingBuffer::new(),
            termios: Termios {
                c_iflag: 0,
                c_oflag: 0,
                c_cflag: 0,
                c_lflag: 0,
                c_line: 0,
                c_cc: [0; NCCS],
                c_ispeed: 0,
                c_ospeed: 0,
            },
            winsize: WinSize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
            line_buffer: [0; 256],
            line_len: 0,
        }
    }

    /// 分配
    pub fn allocate(&mut self) {
        self.allocated = true;
        self.termios = Termios::default();
        self.to_slave.clear();
        self.to_master.clear();
        self.line_len = 0;
    }

    /// 释放
    pub fn release(&mut self) {
        self.allocated = false;
        self.master_open = false;
        self.slave_open = false;
    }

    /// Master 写入 (发送到 Slave)
    pub fn master_write(&mut self, data: &[u8]) -> usize {
        // 输入处理
        let mut processed = 0;
        for &byte in data {
            let byte = self.process_input(byte);
            if let Some(b) = byte {
                if self.to_slave.write(&[b]) > 0 {
                    processed += 1;
                }
            }
        }
        processed
    }

    /// Master 读取 (接收 Slave 输出)
    pub fn master_read(&mut self, buf: &mut [u8]) -> usize {
        self.to_master.read(buf)
    }

    /// Slave 写入 (发送到 Master)
    pub fn slave_write(&mut self, data: &[u8]) -> usize {
        // 输出处理
        let mut written = 0;
        for &byte in data {
            let processed = self.process_output(byte);
            for &b in &processed {
                if b == 0 {
                    break;
                }
                if self.to_master.write(&[b]) > 0 {
                    written += 1;
                }
            }
        }
        written
    }

    /// Slave 读取 (接收 Master 输入)
    pub fn slave_read(&mut self, buf: &mut [u8]) -> usize {
        let canonical = self.termios.c_lflag & LocalFlags::ICANON != 0;

        if canonical {
            // 规范模式：返回完整行
            self.canonical_read(buf)
        } else {
            // 非规范模式：直接返回
            self.to_slave.read(buf)
        }
    }

    /// 规范模式读取
    fn canonical_read(&mut self, buf: &mut [u8]) -> usize {
        // 检查行缓冲区是否有完整行
        if self.line_len == 0 {
            // 从 to_slave 读取到行缓冲区
            let mut temp = [0u8; 1];
            while self.to_slave.read(&mut temp) > 0 {
                let c = temp[0];

                // 检查行结束
                if c == b'\n' || c == self.termios.c_cc[ControlChar::VEOF as usize] {
                    if c == b'\n' {
                        self.line_buffer[self.line_len] = c;
                        self.line_len += 1;
                    }
                    break;
                }

                // 处理退格
                if c == self.termios.c_cc[ControlChar::VERASE as usize] {
                    if self.line_len > 0 {
                        self.line_len -= 1;
                    }
                    continue;
                }

                // 处理 KILL
                if c == self.termios.c_cc[ControlChar::VKILL as usize] {
                    self.line_len = 0;
                    continue;
                }

                // 添加到行缓冲区
                if self.line_len < self.line_buffer.len() {
                    self.line_buffer[self.line_len] = c;
                    self.line_len += 1;
                }
            }
        }

        // 返回行缓冲区内容
        let to_copy = self.line_len.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.line_buffer[..to_copy]);

        // 移动剩余数据
        if to_copy < self.line_len {
            self.line_buffer.copy_within(to_copy..self.line_len, 0);
        }
        self.line_len -= to_copy;

        to_copy
    }

    /// 输入处理
    fn process_input(&self, byte: u8) -> Option<u8> {
        let iflag = self.termios.c_iflag;

        // CR/NL 转换
        if byte == b'\r' {
            if iflag & InputFlags::IGNCR != 0 {
                return None;
            }
            if iflag & InputFlags::ICRNL != 0 {
                return Some(b'\n');
            }
        } else if byte == b'\n' {
            if iflag & InputFlags::INLCR != 0 {
                return Some(b'\r');
            }
        }

        // 去除第 8 位
        if iflag & InputFlags::ISTRIP != 0 {
            return Some(byte & 0x7F);
        }

        Some(byte)
    }

    /// 输出处理
    fn process_output(&self, byte: u8) -> [u8; 2] {
        let oflag = self.termios.c_oflag;

        if oflag & OutputFlags::OPOST == 0 {
            return [byte, 0];
        }

        // NL -> CR-NL
        if byte == b'\n' && oflag & OutputFlags::ONLCR != 0 {
            return [b'\r', b'\n'];
        }

        // CR -> NL
        if byte == b'\r' && oflag & OutputFlags::OCRNL != 0 {
            return [b'\n', 0];
        }

        [byte, 0]
    }
}

// ============================================================================
// PTY 管理器
// ============================================================================

/// PTY 管理器
pub struct PtyManager {
    /// 下一个可用索引
    next_index: AtomicU32,
    /// 已分配数量
    allocated_count: AtomicUsize,
}

impl PtyManager {
    /// 创建新的管理器
    pub const fn new() -> Self {
        Self {
            next_index: AtomicU32::new(0),
            allocated_count: AtomicUsize::new(0),
        }
    }

    /// 分配新的 PTY
    pub fn allocate(&self) -> Option<u32> {
        let count = self.allocated_count.load(Ordering::Relaxed);
        if count >= MAX_PTYS {
            return None;
        }

        let index = self.next_index.fetch_add(1, Ordering::Relaxed);
        if index as usize >= MAX_PTYS {
            self.next_index.fetch_sub(1, Ordering::Relaxed);
            return None;
        }

        self.allocated_count.fetch_add(1, Ordering::Relaxed);
        Some(index)
    }

    /// 释放 PTY
    pub fn release(&self, _index: u32) {
        self.allocated_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// 获取已分配数量
    pub fn count(&self) -> usize {
        self.allocated_count.load(Ordering::Relaxed)
    }
}

// ============================================================================
// 全局状态
// ============================================================================

static PTY_INIT: Once = Once::new();
static PTY_MANAGER: PtyManager = PtyManager::new();

/// 初始化 PTY 子系统
pub fn init() {
    PTY_INIT.call_once(|| {
        // PTY 初始化逻辑
    });
}

/// 检查是否已初始化
pub fn initialized() -> bool {
    PTY_INIT.is_completed()
}

/// 分配新的 PTY
pub fn allocate() -> Option<u32> {
    PTY_MANAGER.allocate()
}

/// 释放 PTY
pub fn release(index: u32) {
    PTY_MANAGER.release(index);
}

/// 获取已分配的 PTY 数量
pub fn count() -> usize {
    PTY_MANAGER.count()
}
