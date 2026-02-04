// ============================================================================
// january_os - DMAR (DMA Remapping Table)
//
// Intel VT-d IOMMU 描述表
// ============================================================================

use super::{SdtHeader, AcpiTable};

/// DMAR 表签名
pub const DMAR_SIGNATURE: &[u8; 4] = b"DMAR";

/// DMAR (DMA Remapping Table)
#[repr(C, packed)]
pub struct Dmar {
    pub header: SdtHeader,
    /// 主机地址宽度 (物理地址位数 - 1)
    pub host_address_width: u8,
    /// 标志
    pub flags: u8,
    /// 保留
    pub reserved: [u8; 10],
    // 后面是变长的重映射结构数组
}

impl AcpiTable for Dmar {
    fn signature() -> &'static [u8; 4] {
        DMAR_SIGNATURE
    }
}

impl Dmar {
    /// 获取支持的物理地址位数
    pub fn physical_address_bits(&self) -> u8 {
        self.host_address_width + 1
    }
    
    /// 是否启用中断重映射
    pub fn interrupt_remapping(&self) -> bool {
        self.flags & 0x01 != 0
    }
    
    /// 是否启用 x2APIC 模式
    pub fn x2apic_opt_out(&self) -> bool {
        self.flags & 0x02 != 0
    }
    
    /// 是否启用 DMA 控制平台可选择
    pub fn dma_control_platform_opt_in(&self) -> bool {
        self.flags & 0x04 != 0
    }
    
    /// 遍历所有条目
    pub fn entries(&self) -> DmarEntryIter {
        let header_size = core::mem::size_of::<Dmar>();
        let total_size = self.header.length as usize;
        let entries_start = self as *const _ as *const u8;
        
        DmarEntryIter {
            current: unsafe { entries_start.add(header_size) },
            end: unsafe { entries_start.add(total_size) },
        }
    }
}

/// DMAR 条目迭代器
pub struct DmarEntryIter {
    current: *const u8,
    end: *const u8,
}

impl Iterator for DmarEntryIter {
    type Item = DmarEntry;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }
        
        unsafe {
            let header = &*(self.current as *const DmarStructHeader);
            
            if header.length < 2 {
                return None;
            }
            
            let entry = match header.struct_type {
                0 => {
                    let drhd = &*(self.current as *const Drhd);
                    DmarEntry::Drhd(DmarDrhd::from_raw(drhd))
                }
                1 => {
                    let rmrr = &*(self.current as *const Rmrr);
                    DmarEntry::Rmrr(DmarRmrr::from_raw(rmrr))
                }
                2 => {
                    let atsr = &*(self.current as *const Atsr);
                    DmarEntry::Atsr(DmarAtsr::from_raw(atsr))
                }
                3 => {
                    let rhsa = &*(self.current as *const Rhsa);
                    DmarEntry::Rhsa(DmarRhsa::from_raw(rhsa))
                }
                4 => {
                    let andd = &*(self.current as *const Andd);
                    DmarEntry::Andd(DmarAndd::from_raw(andd))
                }
                5 => {
                    let satc = &*(self.current as *const Satc);
                    DmarEntry::Satc(DmarSatc::from_raw(satc))
                }
                _ => DmarEntry::Unknown {
                    struct_type: header.struct_type,
                    length: header.length,
                }
            };
            
            self.current = self.current.add(header.length as usize);
            Some(entry)
        }
    }
}

/// DMAR 结构头部
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DmarStructHeader {
    /// 结构类型
    pub struct_type: u16,
    /// 结构长度
    pub length: u16,
}

/// DMAR 条目类型
#[derive(Debug, Clone)]
pub enum DmarEntry {
    /// DMA 重映射硬件单元定义
    Drhd(DmarDrhd),
    /// 保留内存区域报告
    Rmrr(DmarRmrr),
    /// 根端口 ATS 能力报告
    Atsr(DmarAtsr),
    /// 重映射硬件静态亲和性
    Rhsa(DmarRhsa),
    /// ACPI 命名空间设备声明
    Andd(DmarAndd),
    /// 平台 SATC 能力报告
    Satc(DmarSatc),
    /// 未知类型
    Unknown { struct_type: u16, length: u16 },
}

// ============================================================================
// DRHD - DMA Remapping Hardware Unit Definition
// ============================================================================

/// DRHD 原始结构
#[repr(C, packed)]
pub struct Drhd {
    pub header: DmarStructHeader,
    /// 标志
    pub flags: u8,
    /// 大小 (2^N 页)
    pub size: u8,
    /// PCI 段号
    pub segment: u16,
    /// 寄存器基址
    pub register_base: u64,
    // 后面是设备作用域数组
}

/// DRHD 解析结果
#[derive(Debug, Clone)]
pub struct DmarDrhd {
    /// 是否为"包含全部"设备
    pub include_pci_all: bool,
    /// PCI 段号
    pub segment: u16,
    /// 寄存器基址
    pub register_base: u64,
    /// 设备作用域
    pub device_scopes: [DeviceScope; 16],
    /// 设备作用域数量
    pub device_scope_count: usize,
}

impl DmarDrhd {
    fn from_raw(drhd: &Drhd) -> Self {
        let include_pci_all = drhd.flags & 0x01 != 0;
        let segment = drhd.segment;
        let register_base = drhd.register_base;
        
        let mut result = Self {
            include_pci_all,
            segment,
            register_base,
            device_scopes: [DeviceScope::default(); 16],
            device_scope_count: 0,
        };
        
        // 解析设备作用域
        let header_size = core::mem::size_of::<Drhd>();
        let total_size = drhd.header.length as usize;
        let scope_size = total_size - header_size;
        
        if scope_size > 0 {
            let scope_start = unsafe {
                (drhd as *const _ as *const u8).add(header_size)
            };
            result.parse_device_scopes(scope_start, scope_size);
        }
        
        result
    }
    
    fn parse_device_scopes(&mut self, start: *const u8, size: usize) {
        let mut offset = 0;
        while offset < size && self.device_scope_count < 16 {
            unsafe {
                let scope = &*(start.add(offset) as *const DeviceScopeRaw);
                if scope.length < 6 {
                    break;
                }
                
                self.device_scopes[self.device_scope_count] = DeviceScope {
                    scope_type: scope.scope_type,
                    enumeration_id: scope.enumeration_id,
                    start_bus: scope.start_bus,
                };
                self.device_scope_count += 1;
                
                offset += scope.length as usize;
            }
        }
    }
}

/// 设备作用域原始结构
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceScopeRaw {
    /// 类型
    pub scope_type: u8,
    /// 长度
    pub length: u8,
    /// 保留
    pub reserved: u16,
    /// 枚举 ID
    pub enumeration_id: u8,
    /// 起始总线号
    pub start_bus: u8,
    // 后面是路径条目
}

/// 设备作用域
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceScope {
    /// 类型
    pub scope_type: u8,
    /// 枚举 ID
    pub enumeration_id: u8,
    /// 起始总线号
    pub start_bus: u8,
}

/// 设备作用域类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceScopeType {
    /// PCI 端点设备
    PciEndpoint = 1,
    /// PCI 子层次结构
    PciSubHierarchy = 2,
    /// IOAPIC
    IoApic = 3,
    /// MSI 能力 HPET
    MsiCapableHpet = 4,
    /// ACPI 命名空间设备
    AcpiNamespaceDevice = 5,
}

// ============================================================================
// RMRR - Reserved Memory Region Reporting
// ============================================================================

/// RMRR 原始结构
#[repr(C, packed)]
pub struct Rmrr {
    pub header: DmarStructHeader,
    /// 保留
    pub reserved: u16,
    /// PCI 段号
    pub segment: u16,
    /// 区域基址
    pub base_address: u64,
    /// 区域限制地址
    pub limit_address: u64,
    // 后面是设备作用域数组
}

/// RMRR 解析结果
#[derive(Debug, Clone)]
pub struct DmarRmrr {
    /// PCI 段号
    pub segment: u16,
    /// 区域基址
    pub base_address: u64,
    /// 区域限制地址
    pub limit_address: u64,
}

impl DmarRmrr {
    fn from_raw(rmrr: &Rmrr) -> Self {
        Self {
            segment: rmrr.segment,
            base_address: rmrr.base_address,
            limit_address: rmrr.limit_address,
        }
    }
}

// ============================================================================
// ATSR - Root Port ATS Capability Reporting
// ============================================================================

/// ATSR 原始结构
#[repr(C, packed)]
pub struct Atsr {
    pub header: DmarStructHeader,
    /// 标志
    pub flags: u8,
    /// 保留
    pub reserved: u8,
    /// PCI 段号
    pub segment: u16,
}

/// ATSR 解析结果
#[derive(Debug, Clone)]
pub struct DmarAtsr {
    /// 是否为全部端口
    pub all_ports: bool,
    /// PCI 段号
    pub segment: u16,
}

impl DmarAtsr {
    fn from_raw(atsr: &Atsr) -> Self {
        Self {
            all_ports: atsr.flags & 0x01 != 0,
            segment: atsr.segment,
        }
    }
}

// ============================================================================
// RHSA - Remapping Hardware Static Affinity
// ============================================================================

/// RHSA 原始结构
#[repr(C, packed)]
pub struct Rhsa {
    pub header: DmarStructHeader,
    /// 保留
    pub reserved: u32,
    /// 寄存器基址
    pub register_base: u64,
    /// 邻近域
    pub proximity_domain: u32,
}

/// RHSA 解析结果
#[derive(Debug, Clone)]
pub struct DmarRhsa {
    /// 寄存器基址
    pub register_base: u64,
    /// NUMA 邻近域
    pub proximity_domain: u32,
}

impl DmarRhsa {
    fn from_raw(rhsa: &Rhsa) -> Self {
        Self {
            register_base: rhsa.register_base,
            proximity_domain: rhsa.proximity_domain,
        }
    }
}

// ============================================================================
// ANDD - ACPI Namespace Device Declaration
// ============================================================================

/// ANDD 原始结构
#[repr(C, packed)]
pub struct Andd {
    pub header: DmarStructHeader,
    /// 保留
    pub reserved: [u8; 3],
    /// ACPI 设备号
    pub acpi_device_number: u8,
    // 后面是 ACPI 对象名称
}

/// ANDD 解析结果
#[derive(Debug, Clone)]
pub struct DmarAndd {
    /// ACPI 设备号
    pub acpi_device_number: u8,
}

impl DmarAndd {
    fn from_raw(andd: &Andd) -> Self {
        Self {
            acpi_device_number: andd.acpi_device_number,
        }
    }
}

// ============================================================================
// SATC - SoC Integrated Address Translation Cache Reporting
// ============================================================================

/// SATC 原始结构
#[repr(C, packed)]
pub struct Satc {
    pub header: DmarStructHeader,
    /// 标志
    pub flags: u8,
    /// 保留
    pub reserved: u8,
    /// PCI 段号
    pub segment: u16,
}

/// SATC 解析结果
#[derive(Debug, Clone)]
pub struct DmarSatc {
    /// PCI 段号
    pub segment: u16,
}

impl DmarSatc {
    fn from_raw(satc: &Satc) -> Self {
        Self {
            segment: satc.segment,
        }
    }
}

// ============================================================================
// DMAR 解析结果
// ============================================================================

/// DMAR 解析结果
#[derive(Debug, Default)]
pub struct DmarInfo {
    /// 是否找到 DMAR 表
    pub present: bool,
    /// 物理地址位数
    pub physical_address_bits: u8,
    /// 是否支持中断重映射
    pub interrupt_remapping: bool,
    /// DRHD 数量
    pub drhd_count: usize,
    /// DRHD 信息
    pub drhds: [DrhdInfo; 8],
    /// RMRR 数量
    pub rmrr_count: usize,
    /// RMRR 信息
    pub rmrrs: [RmrrInfo; 8],
}

/// DRHD 简化信息
#[derive(Debug, Clone, Copy, Default)]
pub struct DrhdInfo {
    /// 是否为"包含全部"设备
    pub include_pci_all: bool,
    /// PCI 段号
    pub segment: u16,
    /// 寄存器基址
    pub register_base: u64,
}

/// RMRR 简化信息
#[derive(Debug, Clone, Copy, Default)]
pub struct RmrrInfo {
    /// PCI 段号
    pub segment: u16,
    /// 区域基址
    pub base_address: u64,
    /// 区域限制地址
    pub limit_address: u64,
}

/// 解析 DMAR 并提取信息
pub fn parse_dmar(dmar: &Dmar) -> DmarInfo {
    let mut info = DmarInfo {
        present: true,
        physical_address_bits: dmar.physical_address_bits(),
        interrupt_remapping: dmar.interrupt_remapping(),
        ..Default::default()
    };
    
    for entry in dmar.entries() {
        match entry {
            DmarEntry::Drhd(drhd) => {
                if info.drhd_count < 8 {
                    info.drhds[info.drhd_count] = DrhdInfo {
                        include_pci_all: drhd.include_pci_all,
                        segment: drhd.segment,
                        register_base: drhd.register_base,
                    };
                    info.drhd_count += 1;
                }
            }
            DmarEntry::Rmrr(rmrr) => {
                if info.rmrr_count < 8 {
                    info.rmrrs[info.rmrr_count] = RmrrInfo {
                        segment: rmrr.segment,
                        base_address: rmrr.base_address,
                        limit_address: rmrr.limit_address,
                    };
                    info.rmrr_count += 1;
                }
            }
            _ => {}
        }
    }
    
    info
}
