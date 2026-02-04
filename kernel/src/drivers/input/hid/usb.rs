//! USB 核心抽象
//!
//! 定义 USB 设备、端点、描述符等基本类型

use core::ptr;

// ============================================================================
// USB 常量
// ============================================================================

/// USB 类代码 - HID
pub const USB_CLASS_HID: u8 = 0x03;

/// USB 子类代码 - Boot Interface
pub const USB_SUBCLASS_BOOT: u8 = 0x01;

/// USB 协议代码 - 键盘
pub const USB_PROTOCOL_KEYBOARD: u8 = 0x01;

/// USB 协议代码 - 鼠标
pub const USB_PROTOCOL_MOUSE: u8 = 0x02;

/// USB 请求类型
pub const USB_REQ_TYPE_STANDARD: u8 = 0x00;
pub const USB_REQ_TYPE_CLASS: u8 = 0x20;
pub const USB_REQ_TYPE_VENDOR: u8 = 0x40;

/// USB 请求接收者
pub const USB_REQ_RECIPIENT_DEVICE: u8 = 0x00;
pub const USB_REQ_RECIPIENT_INTERFACE: u8 = 0x01;
pub const USB_REQ_RECIPIENT_ENDPOINT: u8 = 0x02;

/// USB 标准请求
pub const USB_REQ_GET_STATUS: u8 = 0x00;
pub const USB_REQ_CLEAR_FEATURE: u8 = 0x01;
pub const USB_REQ_SET_FEATURE: u8 = 0x03;
pub const USB_REQ_SET_ADDRESS: u8 = 0x05;
pub const USB_REQ_GET_DESCRIPTOR: u8 = 0x06;
pub const USB_REQ_SET_DESCRIPTOR: u8 = 0x07;
pub const USB_REQ_GET_CONFIGURATION: u8 = 0x08;
pub const USB_REQ_SET_CONFIGURATION: u8 = 0x09;
pub const USB_REQ_GET_INTERFACE: u8 = 0x0A;
pub const USB_REQ_SET_INTERFACE: u8 = 0x0B;

/// USB 描述符类型
pub const USB_DESC_TYPE_DEVICE: u8 = 0x01;
pub const USB_DESC_TYPE_CONFIGURATION: u8 = 0x02;
pub const USB_DESC_TYPE_STRING: u8 = 0x03;
pub const USB_DESC_TYPE_INTERFACE: u8 = 0x04;
pub const USB_DESC_TYPE_ENDPOINT: u8 = 0x05;
pub const USB_DESC_TYPE_HID: u8 = 0x21;
pub const USB_DESC_TYPE_HID_REPORT: u8 = 0x22;

// ============================================================================
// USB 传输类型
// ============================================================================

/// USB 传输类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbTransferType {
    /// 控制传输
    Control = 0,
    /// 同步传输
    Isochronous = 1,
    /// 批量传输
    Bulk = 2,
    /// 中断传输
    Interrupt = 3,
}

impl UsbTransferType {
    pub fn from_endpoint_attr(attr: u8) -> Self {
        match attr & 0x03 {
            0 => Self::Control,
            1 => Self::Isochronous,
            2 => Self::Bulk,
            3 => Self::Interrupt,
            _ => Self::Control,
        }
    }
}

/// USB 传输方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbDirection {
    /// 主机到设备 (OUT)
    Out = 0,
    /// 设备到主机 (IN)
    In = 1,
}

impl UsbDirection {
    pub fn from_endpoint_addr(addr: u8) -> Self {
        if addr & 0x80 != 0 {
            Self::In
        } else {
            Self::Out
        }
    }
}

// ============================================================================
// USB 速度
// ============================================================================

/// USB 设备速度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbSpeed {
    /// 低速 (1.5 Mbps)
    Low = 0,
    /// 全速 (12 Mbps)
    Full = 1,
    /// 高速 (480 Mbps)
    High = 2,
    /// 超高速 (5 Gbps)
    Super = 3,
    /// 超高速+ (10 Gbps)
    SuperPlus = 4,
}

// ============================================================================
// USB 描述符
// ============================================================================

/// USB 设备描述符
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbDeviceDescriptor {
    /// 描述符长度 (18)
    pub length: u8,
    /// 描述符类型 (0x01)
    pub descriptor_type: u8,
    /// USB 规范版本 (BCD)
    pub usb_version: u16,
    /// 设备类代码
    pub device_class: u8,
    /// 设备子类代码
    pub device_subclass: u8,
    /// 设备协议代码
    pub device_protocol: u8,
    /// 端点 0 最大包大小
    pub max_packet_size_0: u8,
    /// 厂商 ID
    pub vendor_id: u16,
    /// 产品 ID
    pub product_id: u16,
    /// 设备版本 (BCD)
    pub device_version: u16,
    /// 制造商字符串索引
    pub manufacturer_index: u8,
    /// 产品字符串索引
    pub product_index: u8,
    /// 序列号字符串索引
    pub serial_number_index: u8,
    /// 配置数量
    pub num_configurations: u8,
}

/// USB 配置描述符
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbConfigDescriptor {
    /// 描述符长度 (9)
    pub length: u8,
    /// 描述符类型 (0x02)
    pub descriptor_type: u8,
    /// 配置总长度
    pub total_length: u16,
    /// 接口数量
    pub num_interfaces: u8,
    /// 配置值
    pub configuration_value: u8,
    /// 配置字符串索引
    pub configuration_index: u8,
    /// 属性
    pub attributes: u8,
    /// 最大功耗 (2mA 单位)
    pub max_power: u8,
}

/// USB 接口描述符
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbInterfaceDescriptor {
    /// 描述符长度 (9)
    pub length: u8,
    /// 描述符类型 (0x04)
    pub descriptor_type: u8,
    /// 接口号
    pub interface_number: u8,
    /// 备用设置
    pub alternate_setting: u8,
    /// 端点数量
    pub num_endpoints: u8,
    /// 接口类
    pub interface_class: u8,
    /// 接口子类
    pub interface_subclass: u8,
    /// 接口协议
    pub interface_protocol: u8,
    /// 接口字符串索引
    pub interface_index: u8,
}

/// USB 端点描述符
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbEndpointDescriptor {
    /// 描述符长度 (7)
    pub length: u8,
    /// 描述符类型 (0x05)
    pub descriptor_type: u8,
    /// 端点地址
    pub endpoint_address: u8,
    /// 属性
    pub attributes: u8,
    /// 最大包大小
    pub max_packet_size: u16,
    /// 轮询间隔
    pub interval: u8,
}

impl UsbEndpointDescriptor {
    /// 获取端点号
    pub fn endpoint_number(&self) -> u8 {
        self.endpoint_address & 0x0F
    }
    
    /// 获取传输方向
    pub fn direction(&self) -> UsbDirection {
        UsbDirection::from_endpoint_addr(self.endpoint_address)
    }
    
    /// 获取传输类型
    pub fn transfer_type(&self) -> UsbTransferType {
        UsbTransferType::from_endpoint_attr(self.attributes)
    }
}

// ============================================================================
// USB 设备
// ============================================================================

/// USB 设备
pub struct UsbDevice {
    /// 设备地址
    pub address: u8,
    /// 设备速度
    pub speed: UsbSpeed,
    /// 设备描述符
    pub device_desc: UsbDeviceDescriptor,
    /// 当前配置
    pub configuration: u8,
    /// 父集线器端口
    pub parent_port: u8,
}

impl UsbDevice {
    /// 创建新的 USB 设备
    pub fn new(address: u8, speed: UsbSpeed) -> Self {
        Self {
            address,
            speed,
            device_desc: unsafe { core::mem::zeroed() },
            configuration: 0,
            parent_port: 0,
        }
    }
    
    /// 检查是否是 HID 设备
    pub fn is_hid_device(&self) -> bool {
        self.device_desc.device_class == USB_CLASS_HID
    }
}

// ============================================================================
// USB 端点
// ============================================================================

/// USB 端点
pub struct UsbEndpoint {
    /// 设备地址
    pub device_address: u8,
    /// 端点号
    pub endpoint_number: u8,
    /// 传输方向
    pub direction: UsbDirection,
    /// 传输类型
    pub transfer_type: UsbTransferType,
    /// 最大包大小
    pub max_packet_size: u16,
    /// 轮询间隔 (毫秒)
    pub interval: u8,
    /// 数据切换位
    pub toggle: bool,
}

impl UsbEndpoint {
    /// 从端点描述符创建
    pub fn from_descriptor(device_address: u8, desc: &UsbEndpointDescriptor) -> Self {
        Self {
            device_address,
            endpoint_number: desc.endpoint_number(),
            direction: desc.direction(),
            transfer_type: desc.transfer_type(),
            max_packet_size: desc.max_packet_size,
            interval: desc.interval,
            toggle: false,
        }
    }
}

// ============================================================================
// USB 请求
// ============================================================================

/// USB 控制请求
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbSetupPacket {
    /// 请求类型
    pub request_type: u8,
    /// 请求
    pub request: u8,
    /// 值
    pub value: u16,
    /// 索引
    pub index: u16,
    /// 长度
    pub length: u16,
}

impl UsbSetupPacket {
    /// 创建 GET_DESCRIPTOR 请求
    pub fn get_descriptor(desc_type: u8, desc_index: u8, length: u16) -> Self {
        Self {
            request_type: 0x80, // Device-to-host, Standard, Device
            request: USB_REQ_GET_DESCRIPTOR,
            value: ((desc_type as u16) << 8) | (desc_index as u16),
            index: 0,
            length,
        }
    }
    
    /// 创建 SET_CONFIGURATION 请求
    pub fn set_configuration(config: u8) -> Self {
        Self {
            request_type: 0x00, // Host-to-device, Standard, Device
            request: USB_REQ_SET_CONFIGURATION,
            value: config as u16,
            index: 0,
            length: 0,
        }
    }
    
    /// 创建 HID GET_REPORT 请求
    pub fn hid_get_report(report_type: u8, report_id: u8, interface: u16, length: u16) -> Self {
        Self {
            request_type: 0xA1, // Device-to-host, Class, Interface
            request: 0x01, // GET_REPORT
            value: ((report_type as u16) << 8) | (report_id as u16),
            index: interface,
            length,
        }
    }
    
    /// 创建 HID SET_REPORT 请求
    pub fn hid_set_report(report_type: u8, report_id: u8, interface: u16, length: u16) -> Self {
        Self {
            request_type: 0x21, // Host-to-device, Class, Interface
            request: 0x09, // SET_REPORT
            value: ((report_type as u16) << 8) | (report_id as u16),
            index: interface,
            length,
        }
    }
    
    /// 创建 HID SET_IDLE 请求
    pub fn hid_set_idle(duration: u8, report_id: u8, interface: u16) -> Self {
        Self {
            request_type: 0x21, // Host-to-device, Class, Interface
            request: 0x0A, // SET_IDLE
            value: ((duration as u16) << 8) | (report_id as u16),
            index: interface,
            length: 0,
        }
    }
    
    /// 创建 HID SET_PROTOCOL 请求
    pub fn hid_set_protocol(protocol: u8, interface: u16) -> Self {
        Self {
            request_type: 0x21, // Host-to-device, Class, Interface
            request: 0x0B, // SET_PROTOCOL
            value: protocol as u16,
            index: interface,
            length: 0,
        }
    }
}

// ============================================================================
// USB 传输状态
// ============================================================================

/// USB 传输状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbTransferStatus {
    /// 成功
    Success,
    /// 进行中
    Pending,
    /// 设备未响应
    NoResponse,
    /// 数据错误
    DataError,
    /// Stall
    Stall,
    /// 缓冲区溢出
    BufferOverrun,
    /// 缓冲区不足
    BufferUnderrun,
    /// 其他错误
    Error,
}
