// ============================================================================
// january_os - MADT (Multiple APIC Description Table)
//
// 描述系统中的 CPU 和中断控制器
// ============================================================================

use super::{SdtHeader, AcpiTable};

/// MADT 表签名
pub const MADT_SIGNATURE: &[u8; 4] = b"APIC";

/// MADT (Multiple APIC Description Table)
#[repr(C, packed)]
pub struct Madt {
    pub header: SdtHeader,
    /// Local APIC 地址
    pub local_apic_address: u32,
    /// 标志
    pub flags: u32,
    // 后面是变长的中断控制器结构数组
}

impl AcpiTable for Madt {
    fn signature() -> &'static [u8; 4] {
        MADT_SIGNATURE
    }
}

impl Madt {
    /// 获取 Local APIC 物理地址
    pub fn local_apic_addr(&self) -> u64 {
        self.local_apic_address as u64
    }
    
    /// 检查是否有 8259 PIC
    pub fn has_8259_pic(&self) -> bool {
        self.flags & 1 != 0
    }
    
    /// 遍历所有条目
    pub fn entries(&self) -> MadtEntryIter {
        let header_size = core::mem::size_of::<Madt>();
        let total_size = self.header.length as usize;
        let entries_start = self as *const _ as *const u8;
        
        MadtEntryIter {
            current: unsafe { entries_start.add(header_size) },
            end: unsafe { entries_start.add(total_size) },
        }
    }
}

/// MADT 条目迭代器
pub struct MadtEntryIter {
    current: *const u8,
    end: *const u8,
}

impl Iterator for MadtEntryIter {
    type Item = MadtEntry;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }
        
        unsafe {
            let entry_type = *self.current;
            let entry_length = *self.current.add(1);
            
            if entry_length == 0 {
                return None;
            }
            
            let entry = match entry_type {
                0 => {
                    let lapic = &*(self.current as *const LocalApicEntry);
                    MadtEntry::LocalApic(*lapic)
                }
                1 => {
                    let ioapic = &*(self.current as *const IoApicEntry);
                    MadtEntry::IoApic(*ioapic)
                }
                2 => {
                    let iso = &*(self.current as *const InterruptSourceOverride);
                    MadtEntry::InterruptSourceOverride(*iso)
                }
                3 => {
                    let nmi = &*(self.current as *const NmiSource);
                    MadtEntry::NmiSource(*nmi)
                }
                4 => {
                    let lapic_nmi = &*(self.current as *const LocalApicNmi);
                    MadtEntry::LocalApicNmi(*lapic_nmi)
                }
                5 => {
                    let lapic_override = &*(self.current as *const LocalApicAddressOverride);
                    MadtEntry::LocalApicAddressOverride(*lapic_override)
                }
                9 => {
                    let x2apic = &*(self.current as *const LocalX2ApicEntry);
                    MadtEntry::LocalX2Apic(*x2apic)
                }
                _ => MadtEntry::Unknown { 
                    entry_type, 
                    length: entry_length 
                }
            };
            
            self.current = self.current.add(entry_length as usize);
            Some(entry)
        }
    }
}

/// MADT 条目类型
#[derive(Debug, Clone, Copy)]
pub enum MadtEntry {
    /// Local APIC
    LocalApic(LocalApicEntry),
    /// I/O APIC
    IoApic(IoApicEntry),
    /// 中断源覆盖
    InterruptSourceOverride(InterruptSourceOverride),
    /// NMI 源
    NmiSource(NmiSource),
    /// Local APIC NMI
    LocalApicNmi(LocalApicNmi),
    /// Local APIC 地址覆盖
    LocalApicAddressOverride(LocalApicAddressOverride),
    /// Local x2APIC
    LocalX2Apic(LocalX2ApicEntry),
    /// 未知类型
    Unknown { entry_type: u8, length: u8 },
}

/// Local APIC 条目 (Type 0)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApicEntry {
    pub entry_type: u8,
    pub length: u8,
    /// ACPI 处理器 ID
    pub acpi_processor_id: u8,
    /// APIC ID
    pub apic_id: u8,
    /// 标志
    pub flags: u32,
}

impl LocalApicEntry {
    /// CPU 是否启用
    pub fn is_enabled(&self) -> bool {
        self.flags & 1 != 0
    }
    
    /// CPU 是否可以在线启用
    pub fn is_online_capable(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// I/O APIC 条目 (Type 1)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IoApicEntry {
    pub entry_type: u8,
    pub length: u8,
    /// I/O APIC ID
    pub io_apic_id: u8,
    /// 保留
    pub reserved: u8,
    /// I/O APIC 物理地址
    pub io_apic_address: u32,
    /// 全局系统中断基址
    pub global_system_interrupt_base: u32,
}

/// 中断源覆盖 (Type 2)
///
/// 用于将 ISA 中断重映射到不同的 GSI
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptSourceOverride {
    pub entry_type: u8,
    pub length: u8,
    /// 总线 (0 = ISA)
    pub bus: u8,
    /// 源中断（ISA IRQ）
    pub source: u8,
    /// 全局系统中断
    pub global_system_interrupt: u32,
    /// 标志
    pub flags: u16,
}

impl InterruptSourceOverride {
    /// 获取极性
    pub fn polarity(&self) -> Polarity {
        match self.flags & 0x3 {
            0 => Polarity::ConformToSpec,
            1 => Polarity::ActiveHigh,
            3 => Polarity::ActiveLow,
            _ => Polarity::ConformToSpec,
        }
    }
    
    /// 获取触发模式
    pub fn trigger_mode(&self) -> TriggerMode {
        match (self.flags >> 2) & 0x3 {
            0 => TriggerMode::ConformToSpec,
            1 => TriggerMode::Edge,
            3 => TriggerMode::Level,
            _ => TriggerMode::ConformToSpec,
        }
    }
}

/// 极性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    ConformToSpec,
    ActiveHigh,
    ActiveLow,
}

/// 触发模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    ConformToSpec,
    Edge,
    Level,
}

/// NMI 源 (Type 3)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NmiSource {
    pub entry_type: u8,
    pub length: u8,
    /// 标志
    pub flags: u16,
    /// 全局系统中断
    pub global_system_interrupt: u32,
}

/// Local APIC NMI (Type 4)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApicNmi {
    pub entry_type: u8,
    pub length: u8,
    /// ACPI 处理器 ID (0xFF = 所有处理器)
    pub acpi_processor_id: u8,
    /// 标志
    pub flags: u16,
    /// Local APIC LINT# (0 或 1)
    pub lint: u8,
}

/// Local APIC 地址覆盖 (Type 5)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApicAddressOverride {
    pub entry_type: u8,
    pub length: u8,
    /// 保留
    pub reserved: u16,
    /// 64 位 Local APIC 地址
    pub local_apic_address: u64,
}

/// Local x2APIC 条目 (Type 9)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalX2ApicEntry {
    pub entry_type: u8,
    pub length: u8,
    /// 保留
    pub reserved: u16,
    /// x2APIC ID
    pub x2apic_id: u32,
    /// 标志
    pub flags: u32,
    /// ACPI 处理器 UID
    pub acpi_processor_uid: u32,
}

impl LocalX2ApicEntry {
    /// CPU 是否启用
    pub fn is_enabled(&self) -> bool {
        self.flags & 1 != 0
    }
}

// ============================================================================
// MADT 解析结果
// ============================================================================

/// MADT 解析结果
#[derive(Debug)]
pub struct MadtInfo {
    /// Local APIC 地址
    pub local_apic_address: u64,
    /// CPU 数量
    pub cpu_count: usize,
    /// CPU APIC ID 列表
    pub cpu_apic_ids: [u32; 256],
    /// I/O APIC 数量
    pub ioapic_count: usize,
    /// I/O APIC 信息
    pub ioapics: [IoApicInfo; 8],
    /// 中断覆盖数量
    pub override_count: usize,
    /// 中断覆盖信息
    pub overrides: [InterruptOverrideInfo; 32],
}

impl Default for MadtInfo {
    fn default() -> Self {
        Self {
            local_apic_address: 0,
            cpu_count: 0,
            cpu_apic_ids: [0; 256],
            ioapic_count: 0,
            ioapics: [IoApicInfo::default(); 8],
            override_count: 0,
            overrides: [InterruptOverrideInfo::default(); 32],
        }
    }
}

/// I/O APIC 信息
#[derive(Debug, Clone, Copy, Default)]
pub struct IoApicInfo {
    pub id: u8,
    pub address: u32,
    pub gsi_base: u32,
}

/// 中断覆盖信息
#[derive(Debug, Clone, Copy, Default)]
pub struct InterruptOverrideInfo {
    pub source: u8,
    pub gsi: u32,
    pub polarity: u8,
    pub trigger: u8,
}

/// 解析 MADT 并提取信息
pub fn parse_madt(madt: &Madt) -> MadtInfo {
    let mut info = MadtInfo::default();
    
    info.local_apic_address = madt.local_apic_addr();
    
    for entry in madt.entries() {
        match entry {
            MadtEntry::LocalApic(lapic) => {
                if lapic.is_enabled() && info.cpu_count < 256 {
                    info.cpu_apic_ids[info.cpu_count] = lapic.apic_id as u32;
                    info.cpu_count += 1;
                }
            }
            MadtEntry::LocalX2Apic(x2apic) => {
                if x2apic.is_enabled() && info.cpu_count < 256 {
                    info.cpu_apic_ids[info.cpu_count] = x2apic.x2apic_id;
                    info.cpu_count += 1;
                }
            }
            MadtEntry::IoApic(ioapic) => {
                if info.ioapic_count < 8 {
                    info.ioapics[info.ioapic_count] = IoApicInfo {
                        id: ioapic.io_apic_id,
                        address: ioapic.io_apic_address,
                        gsi_base: ioapic.global_system_interrupt_base,
                    };
                    info.ioapic_count += 1;
                }
            }
            MadtEntry::InterruptSourceOverride(iso) => {
                if info.override_count < 32 {
                    info.overrides[info.override_count] = InterruptOverrideInfo {
                        source: iso.source,
                        gsi: iso.global_system_interrupt,
                        polarity: iso.polarity() as u8,
                        trigger: iso.trigger_mode() as u8,
                    };
                    info.override_count += 1;
                }
            }
            MadtEntry::LocalApicAddressOverride(override_entry) => {
                info.local_apic_address = override_entry.local_apic_address;
            }
            _ => {}
        }
    }
    
    info
}
