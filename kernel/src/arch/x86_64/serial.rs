//! 串口驱动
//!
//! 提供基于 COM1 (0x3F8) 的串口输入输出功能。

use core::arch::asm;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// COM1 端口基地址
const COM1: u16 = 0x3F8;

/// 串口寄存器偏移
const DATA: u16 = 0; // 数据寄存器
const IER: u16 = 1; // 中断使能寄存器
const IIR: u16 = 2; // 中断标识寄存器
const FCR: u16 = 2; // FIFO 控制寄存器
const LCR: u16 = 3; // 线路控制寄存器
const MCR: u16 = 4; // 调制解调器控制寄存器
const LSR: u16 = 5; // 线路状态寄存器

/// 线路状态寄存器位
const LSR_DATA_READY: u8 = 0x01;
const LSR_TX_EMPTY: u8 = 0x20;

/// 输入缓冲区
const INPUT_BUFFER_SIZE: usize = 256;
static INPUT_BUFFER: [AtomicU8; INPUT_BUFFER_SIZE] = {
    const INIT: AtomicU8 = AtomicU8::new(0);
    [INIT; INPUT_BUFFER_SIZE]
};
static INPUT_HEAD: AtomicUsize = AtomicUsize::new(0);
static INPUT_TAIL: AtomicUsize = AtomicUsize::new(0);

/// 写入端口
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
}

/// 读取端口
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
    value
}

/// 初始化串口 (仅输出)
pub fn serial_init() {
    unsafe {
        outb(COM1 + IER, 0x00); // 禁用中断
        outb(COM1 + LCR, 0x80); // 启用 DLAB
        outb(COM1 + DATA, 0x03); // 波特率 38400 (低字节)
        outb(COM1 + IER, 0x00); // 波特率 38400 (高字节)
        outb(COM1 + LCR, 0x03); // 8 位，无校验，1 停止位
        outb(COM1 + FCR, 0xC7); // 启用 FIFO，清除，14 字节阈值
        outb(COM1 + MCR, 0x0B); // RTS/DSR 设置
    }
}

/// 启用串口接收中断
pub fn serial_enable_rx_interrupt() {
    unsafe {
        // 启用接收数据中断 (IER bit 0)
        outb(COM1 + IER, 0x01);
    }
}

/// 串口中断处理 - 由中断处理程序调用
pub fn serial_interrupt_handler() {
    unsafe {
        // 读取所有可用数据
        while (inb(COM1 + LSR) & LSR_DATA_READY) != 0 {
            let data = inb(COM1 + DATA);
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

/// 从输入缓冲区读取字符
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

/// 检查是否有输入可用
pub fn serial_has_input() -> bool {
    INPUT_TAIL.load(Ordering::Relaxed) != INPUT_HEAD.load(Ordering::Relaxed)
}

/// 非阻塞读取 (轮询模式)
pub fn serial_try_read() -> Option<u8> {
    unsafe {
        if (inb(COM1 + LSR) & LSR_DATA_READY) != 0 {
            Some(inb(COM1 + DATA))
        } else {
            None
        }
    }
}

/// 写入单个字符到串口
fn serial_write_char(c: u8) {
    unsafe {
        // 等待发送缓冲区空闲
        while (inb(COM1 + 5) & 0x20) == 0 {
            core::hint::spin_loop();
        }
        outb(COM1, c);
    }
}

/// 串口写入器
pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                serial_write_char(b'\r');
            }
            serial_write_char(b);
        }
        Ok(())
    }
}

/// 串口打印宏
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::arch::x86_64::serial::SerialWriter, $($arg)*);
    }};
}

/// 串口打印宏（带换行）
#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n")
    };
    ($($arg:tt)*) => {{
        $crate::serial_print!($($arg)*);
        $crate::serial_print!("\n");
    }};
}

// 导出宏供其他模块使用
pub use crate::{serial_print, serial_println};
