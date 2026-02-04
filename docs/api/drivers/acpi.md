# acpi - ACPI 表解析

ACPI (Advanced Configuration and Power Interface) 提供系统配置和电源管理信息。

## API

### 初始化

```rust
pub fn init(rsdp_addr: u64) -> Result<(), AcpiError>
```

**示例**：
```rust
use kernel::drivers::acpi::init;

// 从 BootInfo 获取 RSDP 地址
let rsdp_addr = boot_info.acpi_rsdp_addr;

if rsdp_addr != 0 {
    init(rsdp_addr)?;
}
```

### 获取表

```rust
pub fn get_table<T>(
    signature: u32
) -> Option<&'static T>
```

**签名常量**：
```rust
pub const MADT_SIGNATURE: u32 = 0x43495041; // "APIC"
pub const DMAR_SIGNATURE: u32 = 0x52414D44; // "DMAR"
pub const SRAT_SIGNATURE: u32 = 0x54415253; // "SRAT"
pub const FADT_SIGNATURE: u32 = 0x50434146; // "FACP"
```

**示例**：
```rust
use kernel::drivers::acpi::{get_table, MADT_SIGNATURE, DMAR_SIGNATURE};

// 获取 MADT
if let Some(madt) = get_table::<Madt>(MADT_SIGNATURE) {
    kprintln!("MADT found: {:#x}", madt as *const _ as u64);
}

// 获取 DMAR
if let Some(dmar) = get_table::<Dmar>(DMAR_SIGNATURE) {
    kprintln!("DMAR found: {:#x}", dmar as *const _ as u64);
}
```

### MADT (APIC 信息)

```rust
pub fn parse_madt(madt: &Madt) -> MadtInfo

pub struct MadtInfo {
    pub local_apic_address: u64,
    pub flags: u32,
    pub cpu_count: usize,
    pub ioapics: Vec<IoApicEntry>,
    pub iso_overrides: Vec<IsoOverrideEntry>,
    pub nmi_sources: Vec<NmiEntry>,
}
```

**示例**：
```rust
use kernel::drivers::acpi::{get_table, MADT_SIGNATURE, parse_madt};

if let Some(madt) = get_table::<Madt>(MADT_SIGNATURE) {
    let info = parse_madt(madt);

    kprintln!("CPUs: {}", info.cpu_count);
    kprintln!("Local APIC: {:#x}", info.local_apic_address);

    for ioapic in &info.ioapics {
        kprintln!("I/O APIC: {:#x}, GSI: {}",
            ioapic.address, ioapic.gsi_base);
    }
}
```

### DMAR (IOMMU 信息)

```rust
pub fn parse_dmar(dmar: &Dmar) -> DmarInfo

pub struct DmarInfo {
    pub host_address_width: u8,
    pub flags: DmarFlags,
    pub drhd_units: Vec<DrhdUnit>,
    pub rmrr_units: Vec<RmrrUnit>,
}
```

**示例**：
```rust
use kernel::drivers::acpi::{get_table, DMAR_SIGNATURE, parse_dmar};

if let Some(dmar) = get_table::<Dmar>(DMAR_SIGNATURE) {
    let info = parse_dmar(dmar);

    kprintln!("Address width: {}", info.host_address_width);

    for drhd in &info.drhd_units {
        kprintln!("IOMMU: {:#x}, segment: {}",
            drhd.register_address,
            drhd.segment_number);
    }
}
```

### SRAT (NUMA 拓扑)

```rust
pub fn parse_srat(srat: &Srat) -> SratInfo

pub struct SratInfo {
    pub memory_affinity: Vec<MemoryAffinity>,
    pub processor_affinity: Vec<ProcessorAffinity>,
}
```

**示例**：
```rust
use kernel::drivers::acpi::{get_table, SRAT_SIGNATURE, parse_srat};

if let Some(srat) = get_table::<Srat>(SRAT_SIGNATURE) {
    let info = parse_srat(srat);

    for affinity in &info.memory_affinity {
        kprintln!("Node {}: {:#x} - {:#x}",
            affinity.proximity_domain,
            affinity.base_address,
            affinity.end_address);
    }
}
```

### 电源管理

```rust
pub fn get_shutdown_info() -> Option<(u16, u16)>
pub fn acpi_shutdown() -> Result<(), &'static str>
```

**关机**：
```rust
use kernel::drivers::acpi::{acpi_shutdown, get_shutdown_info};

// 获取 PM1 寄存器
if let Some((pm1a, pm1b)) = get_shutdown_info() {
    kprintln!("PM1a: {:#x}, PM1b: {:#x}", pm1a, pm1b);
}

// 执行关机
acpi_shutdown()?;
```

## 表结构

### RSDP (Root System Description Pointer)

```rust
#[repr(C, packed)]
pub struct Rsdp {
    pub signature: [u8; 8],      // "RSD PTR "
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_physical_address: u32,
    // ACPI 2.0+
    pub length: u32,
    pub xsdt_physical_address: u64,
    pub extended_checksum: u8,
    // ...
}
```

### SDT (System Description Table) 头部

```rust
#[repr(C, packed)]
pub struct SdtHeader {
    pub signature: [u8; 4],    // 表签名
    pub length: u32,           // 表长度
    pub revision: u8,          // 修订号
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}
```

### MADT (Multiple APIC Description Table)

```rust
#[repr(C, packed)]
pub struct Madt {
    pub header: SdtHeader,
    pub local_apic_address: u32,
    pub flags: u32,
    // Entries follow...
}
```

### DMAR (DMA Remapping Table)

```rust
#[repr(C, packed)]
pub struct Dmar {
    pub header: SdtHeader,
    pub host_address_width: u8,
    pub flags: u8,
    pub reserved: [u8; 10],
    // Remapping structures follow...
}
```

## MADT 条目类型

| 类型 | 名称 | 说明 |
|------|------|------|
| 0 | Processor Local APIC | CPU APIC |
| 1 | I/O APIC | I/O APIC |
| 2 | Interrupt Source Override | IRQ 覆盖 |
| 3 | NMI Source | NMI 源 |
| 4 | Local APIC NMI | 本地 APIC NMI |
| 5 | Local APIC Address Override | APIC 地址覆盖 |

## DMAR 单元类型

| 类型 | 名称 | 说明 |
|------|------|------|
| 0 | DRHD | DMA Remapping Hardware Unit |
| 1 | RMRR | Reserved Memory Region Reporting |
| 2 | ATSR | Root Port ATS Capability Reporting |
| 3 | RHSA | Remapping Hardware Status Affinity |

## 使用示例

### 查找 RSDP

```rust
use kernel::drivers::acpi::find_rsdp;

// 在 UEFI 中从配置表获取
let rsdp_addr = uefi::get_config_table(ACPI2_GUID);
```

### 遍历所有表

```rust
use kernel::drivers::acpi::for_each_table;

for_each_table(|signature, header| {
    kprintln!("Table: {}",
        core::str::from_utf8(&signature).unwrap_or("???"));
    kprintln!("  Address: {:#x}", header as *const _ as u64);
    kprintln!("  Length: {}", header.length);
});
```

### 处理 NUMA 拓扑

```rust
use kernel::drivers::acpi::{get_table, SRAT_SIGNATURE, parse_srat};
use kernel::mm::numa::init_numa;

if let Some(srat) = get_table::<Srat>(SRAT_SIGNATURE) {
    let info = parse_srat(srat);

    // 初始化 NUMA
    init_numa(&info)?;
}
```

## 注意事项

1. **表验证**：使用前应验证校验和
2. **生命周期**：表在系统生命周期内有效
3. **版本**：检查 ACPI 版本 (1.x vs 2.0+)
4. **映射**：表可能需要通过直接映射访问

## 相关文档

- [实现: ACPI 解析](../../implementation/acpi.md)
- [IOMMU API](../mm/iommu.md)
- [NUMA API](../mm/numa.md)
