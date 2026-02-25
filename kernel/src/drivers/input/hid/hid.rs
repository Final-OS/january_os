//! HID (Human Interface Device) 协议实现
//!
//! 解析 HID 描述符和报告

use super::usb::{UsbDevice, UsbEndpoint, UsbSetupPacket};

// ============================================================================
// HID 常量
// ============================================================================

/// HID 描述符类型
pub const HID_DESC_TYPE_HID: u8 = 0x21;
pub const HID_DESC_TYPE_REPORT: u8 = 0x22;
pub const HID_DESC_TYPE_PHYSICAL: u8 = 0x23;

/// HID 请求
pub const HID_REQ_GET_REPORT: u8 = 0x01;
pub const HID_REQ_GET_IDLE: u8 = 0x02;
pub const HID_REQ_GET_PROTOCOL: u8 = 0x03;
pub const HID_REQ_SET_REPORT: u8 = 0x09;
pub const HID_REQ_SET_IDLE: u8 = 0x0A;
pub const HID_REQ_SET_PROTOCOL: u8 = 0x0B;

/// HID 报告类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidReportType {
    /// 输入报告 (设备到主机)
    Input = 1,
    /// 输出报告 (主机到设备)
    Output = 2,
    /// 特性报告 (双向)
    Feature = 3,
}

/// HID 协议
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidProtocol {
    /// Boot 协议 (简化)
    Boot = 0,
    /// Report 协议 (完整)
    Report = 1,
}

// ============================================================================
// HID 描述符
// ============================================================================

/// HID 描述符
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HidDescriptor {
    /// 描述符长度
    pub length: u8,
    /// 描述符类型 (0x21)
    pub descriptor_type: u8,
    /// HID 规范版本 (BCD)
    pub hid_version: u16,
    /// 国家代码
    pub country_code: u8,
    /// 类描述符数量
    pub num_descriptors: u8,
    /// 第一个类描述符类型
    pub descriptor_type_0: u8,
    /// 第一个类描述符长度
    pub descriptor_length_0: u16,
}

// ============================================================================
// HID 报告描述符解析
// ============================================================================

/// HID 报告项目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidItemType {
    /// 主项目
    Main,
    /// 全局项目
    Global,
    /// 局部项目
    Local,
    /// 保留
    Reserved,
}

/// HID 主项目标签
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidMainTag {
    Input = 0x08,
    Output = 0x09,
    Feature = 0x0B,
    Collection = 0x0A,
    EndCollection = 0x0C,
}

/// HID 全局项目标签
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidGlobalTag {
    UsagePage = 0x00,
    LogicalMinimum = 0x01,
    LogicalMaximum = 0x02,
    PhysicalMinimum = 0x03,
    PhysicalMaximum = 0x04,
    UnitExponent = 0x05,
    Unit = 0x06,
    ReportSize = 0x07,
    ReportId = 0x08,
    ReportCount = 0x09,
    Push = 0x0A,
    Pop = 0x0B,
}

/// HID 局部项目标签
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidLocalTag {
    Usage = 0x00,
    UsageMinimum = 0x01,
    UsageMaximum = 0x02,
    DesignatorIndex = 0x03,
    DesignatorMinimum = 0x04,
    DesignatorMaximum = 0x05,
    StringIndex = 0x07,
    StringMinimum = 0x08,
    StringMaximum = 0x09,
    Delimiter = 0x0A,
}

/// HID 使用页
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HidUsagePage {
    GenericDesktop = 0x01,
    Simulation = 0x02,
    VR = 0x03,
    Sport = 0x04,
    Game = 0x05,
    GenericDevice = 0x06,
    Keyboard = 0x07,
    Led = 0x08,
    Button = 0x09,
    Ordinal = 0x0A,
    Telephony = 0x0B,
    Consumer = 0x0C,
    Digitizer = 0x0D,
}

/// HID 通用桌面使用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidGenericDesktopUsage {
    Pointer = 0x01,
    Mouse = 0x02,
    Joystick = 0x04,
    Gamepad = 0x05,
    Keyboard = 0x06,
    Keypad = 0x07,
    MultiAxisController = 0x08,
    X = 0x30,
    Y = 0x31,
    Z = 0x32,
    Rx = 0x33,
    Ry = 0x34,
    Rz = 0x35,
    Slider = 0x36,
    Dial = 0x37,
    Wheel = 0x38,
    HatSwitch = 0x39,
}

/// HID 报告项目
#[derive(Debug, Clone, Copy)]
pub struct HidReportItem {
    /// 项目类型
    pub item_type: HidItemType,
    /// 标签
    pub tag: u8,
    /// 数据大小 (0, 1, 2, 4)
    pub size: u8,
    /// 数据
    pub data: u32,
}

impl HidReportItem {
    /// 从字节解析
    pub fn parse(data: &[u8]) -> Option<(Self, usize)> {
        if data.is_empty() {
            return None;
        }
        
        let prefix = data[0];
        
        // 检查长项目 (0xFE)
        if prefix == 0xFE {
            if data.len() < 3 {
                return None;
            }
            let size = data[1];
            let tag = data[2];
            let item_type = HidItemType::Reserved;
            let consumed = 3 + size as usize;
            if data.len() < consumed {
                return None;
            }

            // 长项目在现代 HID 报告描述符中极少使用。
            // 为保持接口兼容，仅保留前 4 字节数据，其余字节由调用方按 consumed 跳过。
            let payload = &data[3..consumed];
            let mut item_data = 0u32;
            for (idx, b) in payload.iter().take(4).enumerate() {
                item_data |= (*b as u32) << (idx * 8);
            }
            return Some((Self {
                item_type,
                tag,
                size,
                data: item_data,
            }, consumed));
        }
        
        // 短项目
        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 0,
        };
        
        let item_type = match (prefix >> 2) & 0x03 {
            0 => HidItemType::Main,
            1 => HidItemType::Global,
            2 => HidItemType::Local,
            _ => HidItemType::Reserved,
        };
        
        let tag = (prefix >> 4) & 0x0F;
        
        if data.len() < 1 + size as usize {
            return None;
        }
        
        let item_data = match size {
            0 => 0,
            1 => data[1] as u32,
            2 => (data[1] as u32) | ((data[2] as u32) << 8),
            4 => (data[1] as u32) | ((data[2] as u32) << 8) 
                | ((data[3] as u32) << 16) | ((data[4] as u32) << 24),
            _ => 0,
        };
        
        Some((Self {
            item_type,
            tag,
            size,
            data: item_data,
        }, 1 + size as usize))
    }
}

// ============================================================================
// HID 报告
// ============================================================================

/// HID 报告
#[derive(Debug, Clone)]
pub struct HidReport {
    /// 报告 ID (0 表示无 ID)
    pub report_id: u8,
    /// 报告类型
    pub report_type: HidReportType,
    /// 报告数据
    pub data: [u8; 64],
    /// 数据长度
    pub length: usize,
}

impl HidReport {
    /// 创建空报告
    pub const fn empty() -> Self {
        Self {
            report_id: 0,
            report_type: HidReportType::Input,
            data: [0; 64],
            length: 0,
        }
    }
    
    /// 从数据创建
    pub fn from_data(data: &[u8], report_type: HidReportType) -> Self {
        let mut report = Self::empty();
        report.report_type = report_type;
        report.length = data.len().min(64);
        report.data[..report.length].copy_from_slice(&data[..report.length]);
        report
    }
}

// ============================================================================
// HID 设备
// ============================================================================

/// HID 设备
pub struct HidDevice {
    /// USB 设备地址
    pub usb_address: u8,
    /// 接口号
    pub interface: u8,
    /// IN 端点
    pub in_endpoint: Option<u8>,
    /// OUT 端点
    pub out_endpoint: Option<u8>,
    /// 当前协议
    pub protocol: HidProtocol,
    /// 报告描述符长度
    pub report_desc_length: u16,
    /// 轮询间隔 (毫秒)
    pub poll_interval: u8,
}

impl HidDevice {
    /// 创建新的 HID 设备
    pub fn new(usb_address: u8, interface: u8) -> Self {
        Self {
            usb_address,
            interface,
            in_endpoint: None,
            out_endpoint: None,
            protocol: HidProtocol::Report,
            report_desc_length: 0,
            poll_interval: 10,
        }
    }
    
    /// 设置为 Boot 协议
    pub fn set_boot_protocol(&mut self) {
        self.protocol = HidProtocol::Boot;
    }
    
    /// 设置为 Report 协议
    pub fn set_report_protocol(&mut self) {
        self.protocol = HidProtocol::Report;
    }
}

// ============================================================================
// Boot 协议报告格式
// ============================================================================

/// Boot 协议键盘报告 (8 字节)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BootKeyboardReport {
    /// 修饰键位图
    pub modifiers: u8,
    /// 保留
    pub reserved: u8,
    /// 按键码 (最多 6 个)
    pub keycodes: [u8; 6],
}

impl BootKeyboardReport {
    /// 检查是否按下左 Ctrl
    pub fn left_ctrl(&self) -> bool { self.modifiers & 0x01 != 0 }
    /// 检查是否按下左 Shift
    pub fn left_shift(&self) -> bool { self.modifiers & 0x02 != 0 }
    /// 检查是否按下左 Alt
    pub fn left_alt(&self) -> bool { self.modifiers & 0x04 != 0 }
    /// 检查是否按下左 GUI (Win)
    pub fn left_gui(&self) -> bool { self.modifiers & 0x08 != 0 }
    /// 检查是否按下右 Ctrl
    pub fn right_ctrl(&self) -> bool { self.modifiers & 0x10 != 0 }
    /// 检查是否按下右 Shift
    pub fn right_shift(&self) -> bool { self.modifiers & 0x20 != 0 }
    /// 检查是否按下右 Alt
    pub fn right_alt(&self) -> bool { self.modifiers & 0x40 != 0 }
    /// 检查是否按下右 GUI (Win)
    pub fn right_gui(&self) -> bool { self.modifiers & 0x80 != 0 }
    
    /// 检查是否按下任意 Shift
    pub fn shift(&self) -> bool { self.left_shift() || self.right_shift() }
    /// 检查是否按下任意 Ctrl
    pub fn ctrl(&self) -> bool { self.left_ctrl() || self.right_ctrl() }
    /// 检查是否按下任意 Alt
    pub fn alt(&self) -> bool { self.left_alt() || self.right_alt() }
}

/// Boot 协议鼠标报告 (3-4 字节)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BootMouseReport {
    /// 按键位图
    pub buttons: u8,
    /// X 轴位移 (有符号)
    pub x: i8,
    /// Y 轴位移 (有符号)
    pub y: i8,
    /// 滚轮位移 (可选)
    pub wheel: i8,
}

impl BootMouseReport {
    /// 检查左键
    pub fn left_button(&self) -> bool { self.buttons & 0x01 != 0 }
    /// 检查右键
    pub fn right_button(&self) -> bool { self.buttons & 0x02 != 0 }
    /// 检查中键
    pub fn middle_button(&self) -> bool { self.buttons & 0x04 != 0 }
}
