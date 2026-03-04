// ============================================================================
// january_os - PS/2 鼠标驱动
// ============================================================================

use core::sync::atomic::{AtomicI16, AtomicU8, AtomicUsize, Ordering};

use super::{
    read_data as ps2_read_data, send_command, wait_input_ready, wait_output_ready, write_data,
};

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
const PS2_CMD_READ_CONFIG: u8 = 0x20;
const PS2_CMD_WRITE_CONFIG: u8 = 0x60;
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

/// 鼠标设备 ID
static MOUSE_ID: AtomicU8 = AtomicU8::new(0);

// ============================================================================
// 鼠标初始化
// ============================================================================

/// 获取鼠标设备 ID
pub fn device_id() -> u8 {
    MOUSE_ID.load(Ordering::Relaxed)
}

/// 初始化 PS/2 鼠标
pub fn init() {
    // 1. 启用辅助设备端口 (Port 2)
    send_command(PS2_CMD_ENABLE_AUX_DEVICE);
    wait_input_ready();

    // 2. 读取配置字节 (Compaq Status Byte)
    send_command(PS2_CMD_READ_CONFIG);
    wait_output_ready();
    let mut config = ps2_read_data();

    // 3. 修改配置:
    //    - Bit 1: Enable IRQ 12 (Mouse)
    //    - Bit 5: Disable Mouse Clock (0 = Enabled) -> 清除该位以启用时钟
    config |= 0x02; // Enable IRQ 12
    config &= !0x20; // Enable Mouse Clock

    // 4. 写回配置字节
    send_command(PS2_CMD_WRITE_CONFIG);
    wait_input_ready();
    write_data(config);
    wait_input_ready();

    // 5. 复位鼠标
    send_to_mouse(MOUSE_CMD_RESET);
    wait_output_ready();
    let ack = ps2_read_data(); // 0xFA
    wait_output_ready();
    let bat_ok = ps2_read_data(); // 0xAA
    wait_output_ready();
    let device_id = ps2_read_data(); // 0x00
    MOUSE_ID.store(device_id, Ordering::Relaxed);

    if ack != 0xFA || bat_ok != 0xAA {
        // 尝试继续，即使复位返回值不完全符合预期
    }

    // 6. 设置默认参数
    send_to_mouse(MOUSE_CMD_SET_DEFAULTS);
    read_ack();

    // 7. 设置流模式
    send_to_mouse(MOUSE_CMD_SET_STREAM_MODE);
    read_ack();

    // 8. 启用数据报告
    send_to_mouse(MOUSE_CMD_ENABLE);
    read_ack();

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

    // 简单的状态机
    match state {
        0 => {
            // Idle -> Byte1
            // 检查 Byte 1 的 Bit 3 是否为 1 (Always 1)
            // 这有助于同步
            if (data & 0x08) != 0 {
                PACKET_BYTE1.store(data, Ordering::Relaxed);
                PACKET_STATE.store(PacketState::Byte2 as u8, Ordering::Relaxed);
            }
        }
        1 => {
            // Byte1 -> Byte2
            PACKET_BYTE2.store(data, Ordering::Relaxed);
            PACKET_STATE.store(PacketState::Byte3 as u8, Ordering::Relaxed);
        }
        2 => {
            // Byte2 -> Byte3
            PACKET_BYTE3.store(data, Ordering::Relaxed);

            // 处理完整数据包
            process_packet();

            // 回到等待 Byte1
            PACKET_STATE.store(PacketState::Idle as u8, Ordering::Relaxed);
        }
        _ => {
            PACKET_STATE.store(PacketState::Idle as u8, Ordering::Relaxed);
        }
    }
}

/// 处理完整的鼠标数据包
fn process_packet() {
    let byte1 = PACKET_BYTE1.load(Ordering::Relaxed);
    let byte2 = PACKET_BYTE2.load(Ordering::Relaxed);
    let byte3 = PACKET_BYTE3.load(Ordering::Relaxed);

    // 解析按钮状态
    let left = byte1 & 0x01 != 0;
    let right = byte1 & 0x02 != 0;
    let middle = byte1 & 0x04 != 0;

    LEFT_BUTTON.store(left as u8, Ordering::Relaxed);
    MIDDLE_BUTTON.store(middle as u8, Ordering::Relaxed);
    RIGHT_BUTTON.store(right as u8, Ordering::Relaxed);

    // 解析 X/Y 增量
    let x_sign = (byte1 & 0x10) != 0;
    let y_sign = (byte1 & 0x20) != 0;

    let mut dx = byte2 as i16;
    let mut dy = byte3 as i16;

    if x_sign {
        dx |= 0xFF00u16 as i16;
    }
    if y_sign {
        dy |= 0xFF00u16 as i16;
    }

    // PS/2 Y 轴向上为正，屏幕向下为正
    // 许多系统在这里取反 Y 轴

    MOUSE_DELTA_X.store(dx, Ordering::Relaxed);
    MOUSE_DELTA_Y.store(dy, Ordering::Relaxed);

    // 增加事件计数
    EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
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
/// 返回原始 PS/2 数据 (向上为正)
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

/// 读取鼠标数据（远程模式，不通过中断）
pub fn read_mouse_data() -> (bool, bool, bool, i16, i16) {
    // 此函数在中断驱动模式下通常不使用
    // 但如果处于远程模式，可以主动查询

    // 简单实现：返回最后一次中断更新的数据
    let left = left_button();
    let middle = middle_button();
    let right = right_button();
    let dx = delta_x();
    let dy = delta_y();

    (left, middle, right, dx, dy)
}
