// ============================================================================
// january_os - PS/2 鼠标驱动
// ============================================================================

use core::sync::atomic::{AtomicI16, AtomicU8, AtomicUsize, Ordering};

use super::{read_data as ps2_read_data, wait_input_ready, wait_output_ready, write_data, send_command};

// ============================================================================
// PS/2 鼠标命令
// ============================================================================

const MOUSE_CMD_RESET: u8 = 0xFF;
const MOUSE_CMD_RESEND: u8 = 0xFE;
const MOUSE_CMD_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_CMD_DISABLE: u8 = 0xF5;
const MOUSE_CMD_ENABLE: u8 = 0xF4;
const MOUSE_CMD_SET_REMOTE_MODE: u8 = 0xF0;
const MOUSE_CMD_SET_STREAM_MODE: u8 = 0xEA;
const MOUSE_CMD_READ_DATA: u8 = 0xEB;
const MOUSE_CMD_SET_SAMPLE_RATE: u8 = 0xF3;
const MOUSE_CMD_SET_RESOLUTION: u8 = 0xE8;
const MOUSE_CMD_SET_SCALING_2_1: u8 = 0xE7;
const MOUSE_CMD_SET_SCALING_1_1: u8 = 0xE6;

// PS/2 控制器命令
const PS2_CMD_ENABLE_AUX_DEVICE: u8 = 0xA8;
const PS2_CMD_DISABLE_AUX_DEVICE: u8 = 0xA7;
const PS2_CMD_WRITE_TO_AUX: u8 = 0xD4;

// ============================================================================
// 鼠标数据包状态
// ============================================================================

/// 鼠标数据包接收状态
#[derive(Clone, Copy, PartialEq, Eq)]
enum PacketState {
    Idle,
    Byte1,
    Byte2,
    Byte3,
}

static PACKET_STATE: AtomicU8 = AtomicU8::new(PacketState::Idle as u8);

/// 鼠标数据包缓冲
static PACKET_BYTE1: AtomicU8 = AtomicU8::new(0);
static PACKET_BYTE2: AtomicU8 = AtomicU8::new(0);
static PACKET_BYTE3: AtomicU8 = AtomicU8::new(0);

// ============================================================================
// 鼠标状态
// ============================================================================

/// 鼠标按钮状态
static LEFT_BUTTON: AtomicU8 = AtomicU8::new(0);
static MIDDLE_BUTTON: AtomicU8 = AtomicU8::new(0);
static RIGHT_BUTTON: AtomicU8 = AtomicU8::new(0);

/// 鼠标移动增量 (最近一次)
static MOUSE_DELTA_X: AtomicI16 = AtomicI16::new(0);
static MOUSE_DELTA_Y: AtomicI16 = AtomicI16::new(0);

/// 事件计数器
static EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// 初始化状态
static INITIALIZED: AtomicU8 = AtomicU8::new(0);

// ============================================================================
// 鼠标初始化
// ============================================================================

/// 初始化 PS/2 鼠标
pub fn init() {
    // 禁用辅助设备
    send_command(PS2_CMD_DISABLE_AUX_DEVICE);
    wait_input_ready();

    // 发送复位命令到鼠标
    send_to_mouse(MOUSE_CMD_RESET);
    wait_output_ready();
    let ack = ps2_read_data(); // 应该收到 0xFA (ACK)
    wait_output_ready();
    let bat_ok = ps2_read_data(); // 应该收到 0xAA (BAT OK)
    wait_output_ready();
    let device_id = ps2_read_data(); // 应该收到 0x00 (标准鼠标)

    if ack != 0xFA || bat_ok != 0xAA {
        // 初始化失败，但不继续
        return;
    }

    // 设置默认参数
    send_to_mouse(MOUSE_CMD_SET_DEFAULTS);
    read_ack();

    // 启用鼠标
    send_to_mouse(MOUSE_CMD_ENABLE);
    read_ack();

    // 启用辅助设备
    send_command(PS2_CMD_ENABLE_AUX_DEVICE);
    wait_input_ready();

    INITIALIZED.store(1, Ordering::Relaxed);
}

// ============================================================================
// 鼠标中断处理
// ============================================================================

/// 处理 PS/2 鼠标中断 (IRQ12)
///
/// 由中断处理程序调用，传入从数据端口读取的字节
pub fn handle_interrupt(data: u8) {
    if INITIALIZED.load(Ordering::Relaxed) == 0 {
        return;
    }

    let state = PACKET_STATE.load(Ordering::Relaxed);
    let new_state = match state {
        0 => PacketState::Byte1,   // Idle -> Byte1
        1 => PacketState::Byte2,   // Byte1 -> Byte2
        2 => PacketState::Byte3,   // Byte2 -> Byte3
        _ => PacketState::Byte1,    // Byte3 -> Byte1 (意外情况)
    };

    match new_state {
        PacketState::Byte1 => {
            PACKET_BYTE1.store(data, Ordering::Relaxed);
            PACKET_STATE.store(PacketState::Byte1 as u8, Ordering::Relaxed);
        }
        PacketState::Byte2 => {
            PACKET_BYTE2.store(data, Ordering::Relaxed);
            PACKET_STATE.store(PacketState::Byte2 as u8, Ordering::Relaxed);
        }
        PacketState::Byte3 => {
            PACKET_BYTE3.store(data, Ordering::Relaxed);
            PACKET_STATE.store(PacketState::Byte3 as u8, Ordering::Relaxed);

            // 完整数据包接收完成，解析并处理
            process_packet();

            // 重置状态
            PACKET_STATE.store(PacketState::Idle as u8, Ordering::Relaxed);
        }
        _ => {}
    }
}

/// 处理完整的鼠标数据包
fn process_packet() {
    let byte1 = PACKET_BYTE1.load(Ordering::Relaxed);
    let byte2 = PACKET_BYTE2.load(Ordering::Relaxed);
    let byte3 = PACKET_BYTE3.load(Ordering::Relaxed);

    // 检查数据包标志位 (bit 3 of byte1 必须为 1)
    if byte1 & 0x08 == 0 {
        return; // 无效数据包
    }

    // 解析按钮状态
    let left = byte1 & 0x01 != 0;
    let right = byte1 & 0x02 != 0;
    let middle = byte1 & 0x04 != 0;

    LEFT_BUTTON.store(left as u8, Ordering::Relaxed);
    MIDDLE_BUTTON.store(middle as u8, Ordering::Relaxed);
    RIGHT_BUTTON.store(right as u8, Ordering::Relaxed);

    // 解析 X/Y 增量 (使用符号扩展处理 9 位有符号数)
    let delta_x = extend_sign((byte3 & 0x0F) as u16 | ((byte2 as u16) << 4));
    let delta_y = extend_sign(((byte3 & 0xF0) as u16) >> 4 | ((byte1 as u16) << 4));

    MOUSE_DELTA_X.store(delta_x, Ordering::Relaxed);
    MOUSE_DELTA_Y.store(delta_y, Ordering::Relaxed);

    // 增加事件计数
    EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// 将 12 位有符号数扩展为 16 位
fn extend_sign(value: u16) -> i16 {
    if value & 0x800 != 0 {
        (value as i16) | (0xF000u16 as i16)
    } else {
        value as i16
    }
}

// ============================================================================
// 鼠标操作接口
// ============================================================================

/// 检查鼠标是否已初始化
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed) != 0
}

/// 获取左键状态
pub fn left_button() -> bool {
    LEFT_BUTTON.load(Ordering::Relaxed) != 0
}

/// 获取中键状态
pub fn middle_button() -> bool {
    MIDDLE_BUTTON.load(Ordering::Relaxed) != 0
}

/// 获取右键状态
pub fn right_button() -> bool {
    RIGHT_BUTTON.load(Ordering::Relaxed) != 0
}

/// 获取 X 轴移动增量
pub fn delta_x() -> i16 {
    MOUSE_DELTA_X.load(Ordering::Relaxed)
}

/// 获取 Y 轴移动增量
///
/// 注意：PS/2 鼠标 Y 轴方向与屏幕坐标系相反
/// 向上移动鼠标会产生负值
pub fn delta_y() -> i16 {
    MOUSE_DELTA_Y.load(Ordering::Relaxed)
}

/// 获取事件计数
pub fn event_count() -> usize {
    EVENT_COUNT.load(Ordering::Relaxed)
}

/// 检查是否有新事件
pub fn has_event() -> bool {
    event_count() > 0
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 发送命令到鼠标
fn send_to_mouse(cmd: u8) {
    // 先发送"写入辅助设备"命令
    wait_input_ready();
    send_command(PS2_CMD_WRITE_TO_AUX);
    wait_input_ready();

    // 发送鼠标命令
    write_data(cmd);
    wait_input_ready();
}

/// 读取并检查 ACK
fn read_ack() -> bool {
    wait_output_ready();
    let ack = ps2_read_data();
    ack == 0xFA
}

/// 设置鼠标采样率
pub fn set_sample_rate(rate: u8) -> bool {
    if INITIALIZED.load(Ordering::Relaxed) == 0 {
        return false;
    }

    send_to_mouse(MOUSE_CMD_SET_SAMPLE_RATE);
    if !read_ack() {
        return false;
    }

    send_to_mouse(rate);
    read_ack()
}

/// 设置鼠标分辨率
pub fn set_resolution(res: u8) -> bool {
    if INITIALIZED.load(Ordering::Relaxed) == 0 {
        return false;
    }

    // res: 0=1 dpi/count, 1=2 dpi/count, 2=4 dpi/count, 3=8 dpi/count
    send_to_mouse(MOUSE_CMD_SET_RESOLUTION);
    if !read_ack() {
        return false;
    }

    send_to_mouse(res);
    read_ack()
}

/// 读取鼠标数据（远程模式）
pub fn read_mouse_data() -> (bool, bool, bool, i16, i16) {
    if INITIALIZED.load(Ordering::Relaxed) == 0 {
        return (false, false, false, 0, 0);
    }

    send_to_mouse(MOUSE_CMD_READ_DATA);
    if !read_ack() {
        return (false, false, false, 0, 0);
    }

    // 读取 3 字节数据包
    wait_output_ready();
    let byte1 = ps2_read_data();
    wait_output_ready();
    let byte2 = ps2_read_data();
    wait_output_ready();
    let byte3 = ps2_read_data();

    let left = byte1 & 0x01 != 0;
    let middle = byte1 & 0x04 != 0;
    let right = byte1 & 0x02 != 0;

    let delta_x = extend_sign((byte3 & 0x0F) as u16 | ((byte2 as u16) << 4));
    let delta_y = extend_sign(((byte3 & 0xF0) as u16) >> 4 | ((byte1 as u16) << 4));

    (left, middle, right, delta_x, delta_y)
}
