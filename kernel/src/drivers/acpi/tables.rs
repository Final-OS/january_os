// ============================================================================
// january_os - ACPI 基础表结构
// ============================================================================

/// RSDP (Root System Description Pointer)
///
/// ACPI 的入口点，包含指向 RSDT/XSDT 的指针
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    /// 签名 "RSD PTR "
    pub signature: [u8; 8],
    /// 校验和（前 20 字节）
    pub checksum: u8,
    /// OEM ID
    pub oem_id: [u8; 6],
    /// ACPI 版本 (0 = 1.0, 2 = 2.0+)
    pub revision: u8,
    /// RSDT 物理地址（ACPI 1.0）
    pub rsdt_address: u32,
    
    // === ACPI 2.0+ 扩展字段 ===
    /// 结构长度
    pub length: u32,
    /// XSDT 物理地址（64位）
    pub xsdt_address: u64,
    /// 扩展校验和
    pub extended_checksum: u8,
    /// 保留
    pub reserved: [u8; 3],
}

impl Rsdp {
    /// 从物理地址读取并验证 RSDP
    pub unsafe fn from_addr(phys_addr: u64) -> Result<&'static Self, &'static str> {
        let virt_addr = phys_addr + crate::config::DIRECT_MAP_OFFSET;
        let rsdp = &*(virt_addr as *const Rsdp);
        
        // 验证签名
        if &rsdp.signature != b"RSD PTR " {
            return Err("Invalid RSDP signature");
        }
        
        // 验证校验和（前 20 字节）
        let mut sum: u8 = 0;
        let bytes = core::slice::from_raw_parts(virt_addr as *const u8, 20);
        for &b in bytes {
            sum = sum.wrapping_add(b);
        }
        if sum != 0 {
            return Err("Invalid RSDP checksum");
        }
        
        // ACPI 2.0+ 额外验证
        if rsdp.revision >= 2 {
            let mut ext_sum: u8 = 0;
            let ext_bytes = core::slice::from_raw_parts(
                virt_addr as *const u8,
                rsdp.length as usize
            );
            for &b in ext_bytes {
                ext_sum = ext_sum.wrapping_add(b);
            }
            if ext_sum != 0 {
                return Err("Invalid RSDP extended checksum");
            }
        }
        
        Ok(rsdp)
    }
}

/// SDT Header (System Description Table Header)
///
/// 所有 ACPI 表的通用头部
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SdtHeader {
    /// 表签名（4 字节 ASCII）
    pub signature: [u8; 4],
    /// 表长度（包括头部）
    pub length: u32,
    /// ACPI 规范版本
    pub revision: u8,
    /// 校验和
    pub checksum: u8,
    /// OEM ID
    pub oem_id: [u8; 6],
    /// OEM 表 ID
    pub oem_table_id: [u8; 8],
    /// OEM 版本
    pub oem_revision: u32,
    /// 创建者 ID
    pub creator_id: u32,
    /// 创建者版本
    pub creator_revision: u32,
}

impl SdtHeader {
    /// 获取签名字符串
    pub fn signature_str(&self) -> &str {
        core::str::from_utf8(&self.signature).unwrap_or("????")
    }
    
    /// 获取 OEM ID 字符串
    pub fn oem_id_str(&self) -> &str {
        core::str::from_utf8(&self.oem_id).unwrap_or("??????")
    }
}

/// XSDT (Extended System Description Table)
///
/// 包含指向其他 ACPI 表的 64 位指针数组
#[repr(C, packed)]
pub struct Xsdt {
    pub header: SdtHeader,
    // 后面是 u64 指针数组，长度由 header.length 决定
}

impl Xsdt {
    /// 获取表条目数量
    pub fn entry_count(&self) -> usize {
        let entries_size = self.header.length as usize - core::mem::size_of::<SdtHeader>();
        entries_size / 8
    }
    
    /// 获取表条目
    pub unsafe fn entry(&self, index: usize) -> u64 {
        if index >= self.entry_count() {
            return 0;
        }
        let entries = (self as *const _ as *const u8)
            .add(core::mem::size_of::<SdtHeader>()) as *const u64;
        *entries.add(index)
    }
}

/// RSDT (Root System Description Table)
///
/// ACPI 1.0 版本，包含 32 位指针
#[repr(C, packed)]
pub struct Rsdt {
    pub header: SdtHeader,
    // 后面是 u32 指针数组
}

impl Rsdt {
    /// 获取表条目数量
    pub fn entry_count(&self) -> usize {
        let entries_size = self.header.length as usize - core::mem::size_of::<SdtHeader>();
        entries_size / 4
    }
    
    /// 获取表条目
    pub unsafe fn entry(&self, index: usize) -> u32 {
        if index >= self.entry_count() {
            return 0;
        }
        let entries = (self as *const _ as *const u8)
            .add(core::mem::size_of::<SdtHeader>()) as *const u32;
        *entries.add(index)
    }
}

/// Generic Address Structure (GAS)
///
/// ACPI 通用地址结构，用于描述寄存器位置
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GenericAddress {
    /// 地址空间 ID
    pub address_space: u8,
    /// 寄存器位宽
    pub bit_width: u8,
    /// 寄存器位偏移
    pub bit_offset: u8,
    /// 访问大小
    pub access_size: u8,
    /// 地址
    pub address: u64,
}

/// 地址空间 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AddressSpace {
    /// 系统内存
    SystemMemory = 0,
    /// 系统 I/O
    SystemIo = 1,
    /// PCI 配置空间
    PciConfig = 2,
    /// 嵌入式控制器
    EmbeddedController = 3,
    /// SMBus
    SmBus = 4,
    /// 系统 CMOS
    SystemCmos = 5,
    /// PCI BAR 目标
    PciBarTarget = 6,
    /// IPMI
    Ipmi = 7,
    /// GPIO
    Gpio = 8,
    /// 通用串行总线
    GenericSerialBus = 9,
    /// 平台通信通道
    Pcc = 0x0A,
    /// 功能固定硬件
    FunctionalFixed = 0x7F,
}
