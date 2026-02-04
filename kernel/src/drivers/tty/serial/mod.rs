//! 串口 (UART) 驱动
//!
//! 支持 16550 兼容 UART，提供 COM1-COM4

use core::arch::asm;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

// ============================================================================
// 端口定义
// ============================================================================

/// COM1 端口基地址
pub const COM1_PORT: u16 = 0x3F8;
/// COM2 端口基地址
pub const COM2_PORT: u16 = 0x2F8;
/// COM3 端口基地址
pub const COM3_PORT: u16 = 0x3E8;
/// COM4 端口基地址
pub const COM4_PORT: u16 = 0x2E8;

// ============================================================================
// 寄存器偏移
// ============================================================================

/// 数据寄存器 (DLAB=0 读/写)
const REG_DATA: u16 = 0;
/// 中断使能寄存器 (DLAB=0)
const REG_IER: u16 = 1;
/// 中断标识寄存器 (读)
const REG_IIR: u16 = 2;
/// FIFO 控制寄存器 (写)
const REG_FCR: u16 = 2;
/// 线路控制寄存器
const REG_LCR: u16 = 3;
/// 调制解调器控制寄存器
const REG_MCR: u16 = 4;
/// 线路状态寄存器
const REG_LSR: u16 = 5;
/// 调制解调器状态寄存器
const REG_MSR: u16 = 6;
/// Scratch 寄存器
const REG_SCRATCH: u16 = 7;

// 除数锁存器 (DLAB=1)
const REG_DLL: u16 = 0; // 低字节
const REG_DLH: u16 = 1; // 高字节

// ============================================================================
// 线路状态寄存器位
// ============================================================================

/// 数据就绪
const LSR_DATA_READY: u8 = 0x01;
/// 发送保持寄存器空
const LSR_THR_EMPTY: u8 = 0x20;
/// 发送器空
const LSR_TX_EMPTY: u8 = 0x40;

// ============================================================================
// 中断使能寄存器位
// ============================================================================

/// 接收数据可用中断
const IER_RDA: u8 = 0x01;
/// 发送保持寄存器空中断
const IER_THRE: u8 = 0x02;
/// 接收线路状态中断
const IER_RLS: u8 = 0x04;
/// 调制解调器状态中断
const IER_MS: u8 = 0x08;

// ============================================================================
// 输入缓冲区
// ============================================================================

const INPUT_BUFFER_SIZE: usize = 256;

static INPUT_BUFFER: [AtomicU8; INPUT_BUFFER_SIZE] = {
    const INIT: AtomicU8 = AtomicU8::new(0);
    [INIT; INPUT_BUFFER_SIZE]
};
static INPUT_HEAD: AtomicUsize = AtomicUsize::new(0);
static INPUT_TAIL: AtomicUsize = AtomicUsize::new(0);

// ============================================================================
// Serial 结构体
// ============================================================================

/// 串口设备
pub struct Serial {
    /// 端口基地址
    port: u16,
}

impl Serial {
    /// 创建新的串口实例
    pub const fn new(port: u16) -> Self {
        Self { port }
    }
    
    /// 初始化串口
    pub fn init(&self, baud_rate: u32) {
        let divisor = 115200 / baud_rate;
        
        unsafe {
            // 禁用中断
            outb(self.port + REG_IER, 0x00);
            
            // 设置 DLAB
            outb(self.port + REG_LCR, 0x80);
            
            // 设置波特率除数
            outb(self.port + REG_DLL, (divisor & 0xFF) as u8);
            outb(self.port + REG_DLH, ((divisor >> 8) & 0xFF) as u8);
            
            // 8 位数据，1 停止位，无校验
            outb(self.port + REG_LCR, 0x03);
            
            // 启用 FIFO，清除，14 字节阈值
            outb(self.port + REG_FCR, 0xC7);
            
            // 启用 DTR, RTS, OUT2
            outb(self.port + REG_MCR, 0x0B);
        }
    }
    
    /// 发送单个字节
    pub fn write_byte(&self, byte: u8) {
        unsafe {
            // 等待发送缓冲区空闲
            while (inb(self.port + REG_LSR) & LSR_THR_EMPTY) == 0 {
                core::hint::spin_loop();
            }
            outb(self.port + REG_DATA, byte);
        }
    }
    
    /// 读取单个字节 (阻塞)
    pub fn read_byte(&self) -> u8 {
        unsafe {
            while (inb(self.port + REG_LSR) & LSR_DATA_READY) == 0 {
                core::hint::spin_loop();
            }
            inb(self.port + REG_DATA)
        }
    }
    
    /// 尝试读取字节 (非阻塞)
    pub fn try_read_byte(&self) -> Option<u8> {
        unsafe {
            if (inb(self.port + REG_LSR) & LSR_DATA_READY) != 0 {
                Some(inb(self.port + REG_DATA))
            } else {
                None
            }
        }
    }
    
    /// 检查是否有数据可读
    pub fn has_data(&self) -> bool {
        unsafe { (inb(self.port + REG_LSR) & LSR_DATA_READY) != 0 }
    }
    
    /// 启用接收中断
    pub fn enable_rx_interrupt(&self) {
        unsafe {
            outb(self.port + REG_IER, IER_RDA);
        }
    }
    
    /// 禁用所有中断
    pub fn disable_interrupts(&self) {
        unsafe {
            outb(self.port + REG_IER, 0x00);
        }
    }
}

impl Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
        Ok(())
    }
}

// ============================================================================
// 全局 COM1 接口
// ============================================================================

/// 初始化 COM1
pub fn serial_init() {
    let com1 = Serial::new(COM1_PORT);
    com1.init(38400);
}

/// 写入字符到 COM1
pub fn serial_write(byte: u8) {
    unsafe {
        while (inb(COM1_PORT + REG_LSR) & LSR_THR_EMPTY) == 0 {
            core::hint::spin_loop();
        }
        outb(COM1_PORT + REG_DATA, byte);
    }
}

/// 读取字符从 COM1 (阻塞)
pub fn serial_read() -> u8 {
    unsafe {
        while (inb(COM1_PORT + REG_LSR) & LSR_DATA_READY) == 0 {
            core::hint::spin_loop();
        }
        inb(COM1_PORT + REG_DATA)
    }
}

/// 启用 COM1 接收中断
pub fn serial_enable_rx_interrupt() {
    unsafe {
        outb(COM1_PORT + REG_IER, IER_RDA);
    }
}

/// COM1 中断处理
pub fn serial_interrupt_handler() {
    unsafe {
        while (inb(COM1_PORT + REG_LSR) & LSR_DATA_READY) != 0 {
            let data = inb(COM1_PORT + REG_DATA);
            push_input(data);
        }
    }
}

/// 将字符放入输入缓冲区
fn push_input(c: u8) {
    let head = INPUT_HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % INPUT_BUFFER_SIZE;
    
    if next_head != INPUT_TAIL.load(Ordering::Relaxed) {
        INPUT_BUFFER[head].store(c, Ordering::Relaxed);
        INPUT_HEAD.store(next_head, Ordering::Relaxed);
    }
}

/// 从缓冲区读取字符
pub fn serial_read_char() -> Option<u8> {
    let tail = INPUT_TAIL.load(Ordering::Relaxed);
    let head = INPUT_HEAD.load(Ordering::Relaxed);
    
    if tail == head {
        return None;
    }
    
    let c = INPUT_BUFFER[tail].load(Ordering::Relaxed);
    INPUT_TAIL.store((tail + 1) % INPUT_BUFFER_SIZE, Ordering::Relaxed);
    Some(c)
}

/// 检查是否有输入
pub fn serial_has_input() -> bool {
    INPUT_TAIL.load(Ordering::Relaxed) != INPUT_HEAD.load(Ordering::Relaxed)
}

/// 轮询读取
pub fn serial_try_read() -> Option<u8> {
    unsafe {
        if (inb(COM1_PORT + REG_LSR) & LSR_DATA_READY) != 0 {
            Some(inb(COM1_PORT + REG_DATA))
        } else {
            None
        }
    }
}

// ============================================================================
// 串口写入器 (用于 kprint!)
// ============================================================================

/// 串口写入器
pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                serial_write(b'\r');
            }
            serial_write(b);
        }
        Ok(())
    }
}

// ============================================================================
// 端口 I/O
// ============================================================================

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
    value
}
