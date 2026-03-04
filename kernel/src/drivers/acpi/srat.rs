// ============================================================================
// january_os - SRAT (System Resource Affinity Table)
//
// NUMA 内存和 CPU 亲和性信息
// ============================================================================

use super::{AcpiTable, SdtHeader};

/// SRAT 表签名
pub const SRAT_SIGNATURE: &[u8; 4] = b"SRAT";

/// SRAT (System Resource Affinity Table)
#[repr(C, packed)]
pub struct Srat {
    pub header: SdtHeader,
    /// 保留 (表版本)
    pub reserved1: u32,
    /// 保留
    pub reserved2: u64,
    // 后面是变长的亲和性结构数组
}

impl AcpiTable for Srat {
    fn signature() -> &'static [u8; 4] {
        SRAT_SIGNATURE
    }
}

impl Srat {
    /// 遍历所有条目
    pub fn entries(&self) -> SratEntryIter {
        let header_size = core::mem::size_of::<Srat>();
        let total_size = self.header.length as usize;
        let entries_start = self as *const _ as *const u8;

        SratEntryIter {
            current: unsafe { entries_start.add(header_size) },
            end: unsafe { entries_start.add(total_size) },
        }
    }
}

/// SRAT 条目迭代器
pub struct SratEntryIter {
    current: *const u8,
    end: *const u8,
}

impl Iterator for SratEntryIter {
    type Item = SratEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }

        unsafe {
            let entry_type = *self.current;
            let entry_length = *self.current.add(1);

            if entry_length < 2 {
                return None;
            }

            let entry = match entry_type {
                0 => {
                    let lapic = &*(self.current as *const LocalApicAffinity);
                    SratEntry::LocalApicAffinity(*lapic)
                }
                1 => {
                    let mem = &*(self.current as *const MemoryAffinity);
                    SratEntry::MemoryAffinity(*mem)
                }
                2 => {
                    let x2apic = &*(self.current as *const X2ApicAffinity);
                    SratEntry::X2ApicAffinity(*x2apic)
                }
                3 => {
                    let gicc = &*(self.current as *const GiccAffinity);
                    SratEntry::GiccAffinity(*gicc)
                }
                4 => {
                    let gic_its = &*(self.current as *const GicItsAffinity);
                    SratEntry::GicItsAffinity(*gic_its)
                }
                5 => {
                    let generic = &*(self.current as *const GenericInitiatorAffinity);
                    SratEntry::GenericInitiatorAffinity(*generic)
                }
                _ => SratEntry::Unknown {
                    entry_type,
                    length: entry_length,
                },
            };

            self.current = self.current.add(entry_length as usize);
            Some(entry)
        }
    }
}

/// SRAT 条目类型
#[derive(Debug, Clone, Copy)]
pub enum SratEntry {
    /// Local APIC 亲和性
    LocalApicAffinity(LocalApicAffinity),
    /// 内存亲和性
    MemoryAffinity(MemoryAffinity),
    /// x2APIC 亲和性
    X2ApicAffinity(X2ApicAffinity),
    /// GICC 亲和性 (ARM)
    GiccAffinity(GiccAffinity),
    /// GIC ITS 亲和性 (ARM)
    GicItsAffinity(GicItsAffinity),
    /// 通用初始化器亲和性
    GenericInitiatorAffinity(GenericInitiatorAffinity),
    /// 未知类型
    Unknown { entry_type: u8, length: u8 },
}

/// Local APIC 亲和性 (Type 0)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApicAffinity {
    pub entry_type: u8,
    pub length: u8,
    /// 邻近域低 8 位
    pub proximity_domain_lo: u8,
    /// APIC ID
    pub apic_id: u8,
    /// 标志
    pub flags: u32,
    /// Local SAPIC EID
    pub local_sapic_eid: u8,
    /// 邻近域高 24 位
    pub proximity_domain_hi: [u8; 3],
    /// 时钟域
    pub clock_domain: u32,
}

impl LocalApicAffinity {
    /// 获取完整的邻近域 ID
    pub fn proximity_domain(&self) -> u32 {
        self.proximity_domain_lo as u32
            | ((self.proximity_domain_hi[0] as u32) << 8)
            | ((self.proximity_domain_hi[1] as u32) << 16)
            | ((self.proximity_domain_hi[2] as u32) << 24)
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// 内存亲和性 (Type 1)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryAffinity {
    pub entry_type: u8,
    pub length: u8,
    /// 邻近域
    pub proximity_domain: u32,
    /// 保留
    pub reserved1: u16,
    /// 基址低 32 位
    pub base_address_lo: u32,
    /// 基址高 32 位
    pub base_address_hi: u32,
    /// 长度低 32 位
    pub length_lo: u32,
    /// 长度高 32 位
    pub length_hi: u32,
    /// 保留
    pub reserved2: u32,
    /// 标志
    pub flags: u32,
    /// 保留
    pub reserved3: u64,
}

impl MemoryAffinity {
    /// 获取基址
    pub fn base_address(&self) -> u64 {
        (self.base_address_lo as u64) | ((self.base_address_hi as u64) << 32)
    }

    /// 获取长度
    pub fn length(&self) -> u64 {
        (self.length_lo as u64) | ((self.length_hi as u64) << 32)
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.flags & 1 != 0
    }

    /// 是否可热插拔
    pub fn is_hotpluggable(&self) -> bool {
        self.flags & 2 != 0
    }

    /// 是否为非易失性内存
    pub fn is_non_volatile(&self) -> bool {
        self.flags & 4 != 0
    }
}

/// x2APIC 亲和性 (Type 2)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct X2ApicAffinity {
    pub entry_type: u8,
    pub length: u8,
    /// 保留
    pub reserved1: u16,
    /// 邻近域
    pub proximity_domain: u32,
    /// x2APIC ID
    pub x2apic_id: u32,
    /// 标志
    pub flags: u32,
    /// 时钟域
    pub clock_domain: u32,
    /// 保留
    pub reserved2: u32,
}

impl X2ApicAffinity {
    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// GICC 亲和性 (Type 3, ARM)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GiccAffinity {
    pub entry_type: u8,
    pub length: u8,
    /// 邻近域
    pub proximity_domain: u32,
    /// ACPI 处理器 UID
    pub acpi_processor_uid: u32,
    /// 标志
    pub flags: u32,
    /// 时钟域
    pub clock_domain: u32,
}

impl GiccAffinity {
    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// GIC ITS 亲和性 (Type 4, ARM)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GicItsAffinity {
    pub entry_type: u8,
    pub length: u8,
    /// 邻近域
    pub proximity_domain: u32,
    /// 保留
    pub reserved: u16,
    /// ITS ID
    pub its_id: u32,
}

/// 通用初始化器亲和性 (Type 5)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GenericInitiatorAffinity {
    pub entry_type: u8,
    pub length: u8,
    /// 保留
    pub reserved1: u8,
    /// 设备句柄类型
    pub device_handle_type: u8,
    /// 邻近域
    pub proximity_domain: u32,
    /// 设备句柄
    pub device_handle: [u8; 16],
    /// 标志
    pub flags: u32,
    /// 保留
    pub reserved2: u32,
}

// ============================================================================
// SRAT 解析结果
// ============================================================================

/// SRAT 解析结果
#[derive(Debug)]
pub struct SratInfo {
    /// 是否找到 SRAT 表
    pub present: bool,
    /// NUMA 节点数量
    pub node_count: usize,
    /// 最大邻近域 ID
    pub max_proximity_domain: u32,
    /// CPU 亲和性数量
    pub cpu_affinity_count: usize,
    /// CPU 亲和性信息
    pub cpu_affinities: [CpuAffinityInfo; 256],
    /// 内存亲和性数量
    pub memory_affinity_count: usize,
    /// 内存亲和性信息
    pub memory_affinities: [MemoryAffinityInfo; 64],
}

impl Default for SratInfo {
    fn default() -> Self {
        Self {
            present: false,
            node_count: 0,
            max_proximity_domain: 0,
            cpu_affinity_count: 0,
            cpu_affinities: [CpuAffinityInfo::default(); 256],
            memory_affinity_count: 0,
            memory_affinities: [MemoryAffinityInfo::default(); 64],
        }
    }
}

/// CPU 亲和性信息
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuAffinityInfo {
    /// APIC ID
    pub apic_id: u32,
    /// 邻近域
    pub proximity_domain: u32,
}

/// 内存亲和性信息
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryAffinityInfo {
    /// 邻近域
    pub proximity_domain: u32,
    /// 基址
    pub base_address: u64,
    /// 长度
    pub length: u64,
    /// 是否可热插拔
    pub hotpluggable: bool,
}

/// 解析 SRAT 并提取信息
pub fn parse_srat(srat: &Srat) -> SratInfo {
    let mut info = SratInfo {
        present: true,
        ..Default::default()
    };

    let mut seen_domains = [false; 256];

    for entry in srat.entries() {
        match entry {
            SratEntry::LocalApicAffinity(lapic) => {
                if lapic.is_enabled() && info.cpu_affinity_count < 256 {
                    let domain = lapic.proximity_domain();
                    info.cpu_affinities[info.cpu_affinity_count] = CpuAffinityInfo {
                        apic_id: lapic.apic_id as u32,
                        proximity_domain: domain,
                    };
                    info.cpu_affinity_count += 1;

                    if domain < 256 {
                        seen_domains[domain as usize] = true;
                    }
                    if domain > info.max_proximity_domain {
                        info.max_proximity_domain = domain;
                    }
                }
            }
            SratEntry::X2ApicAffinity(x2apic) => {
                if x2apic.is_enabled() && info.cpu_affinity_count < 256 {
                    let domain = x2apic.proximity_domain;
                    info.cpu_affinities[info.cpu_affinity_count] = CpuAffinityInfo {
                        apic_id: x2apic.x2apic_id,
                        proximity_domain: domain,
                    };
                    info.cpu_affinity_count += 1;

                    if domain < 256 {
                        seen_domains[domain as usize] = true;
                    }
                    if domain > info.max_proximity_domain {
                        info.max_proximity_domain = domain;
                    }
                }
            }
            SratEntry::MemoryAffinity(mem) => {
                if mem.is_enabled() && info.memory_affinity_count < 64 {
                    let domain = mem.proximity_domain;
                    info.memory_affinities[info.memory_affinity_count] = MemoryAffinityInfo {
                        proximity_domain: domain,
                        base_address: mem.base_address(),
                        length: mem.length(),
                        hotpluggable: mem.is_hotpluggable(),
                    };
                    info.memory_affinity_count += 1;

                    if domain < 256 {
                        seen_domains[domain as usize] = true;
                    }
                    if domain > info.max_proximity_domain {
                        info.max_proximity_domain = domain;
                    }
                }
            }
            _ => {}
        }
    }

    // 计算节点数量
    for seen in seen_domains.iter() {
        if *seen {
            info.node_count += 1;
        }
    }

    info
}
