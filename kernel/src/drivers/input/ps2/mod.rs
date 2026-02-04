//! PS/2 控制器驱动
//!
//! 支持 PS/2 键盘和鼠标

pub mod keyboard;
pub mod mouse;

// PS/2 控制器端口
pub const DATA_PORT: u16 = 0x60;
pub const STATUS_PORT: u16 = 0x64;
pub const COMMAND_PORT: u16 = 0x64;

// 状态寄存器位
pub const STATUS_OUTPUT_FULL: u8 = 0x01;
pub const STATUS_INPUT_FULL: u8 = 0x02;

/// 读取 PS/2 数据端口
#[inline]
pub fn read_data() -> u8 {
    unsafe {
        let value: u8;
        core::arch::asm!("in al, dx", out("al") value, in("dx") DATA_PORT);
        value
    }
}

/// 写入 PS/2 数据端口
#[inline]
pub fn write_data(value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") DATA_PORT, in("al") value);
    }
}

/// 读取 PS/2 状态寄存器
#[inline]
pub fn read_status() -> u8 {
    unsafe {
        let value: u8;
        core::arch::asm!("in al, dx", out("al") value, in("dx") STATUS_PORT);
        value
    }
}

/// 发送命令到 PS/2 控制器
#[inline]
pub fn send_command(cmd: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") COMMAND_PORT, in("al") cmd);
    }
}

/// 等待输入缓冲区为空
pub fn wait_input_ready() {
    for _ in 0..10000 {
        if (read_status() & STATUS_INPUT_FULL) == 0 {
            return;
        }
        core::hint::spin_loop();
    }
}

/// 等待输出缓冲区有数据
pub fn wait_output_ready() {
    for _ in 0..10000 {
        if (read_status() & STATUS_OUTPUT_FULL) != 0 {
            return;
        }
        core::hint::spin_loop();
    }
}
