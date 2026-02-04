# ACPI 解析

ACPI (Advanced Configuration and Power Interface) 提供系统配置和电源管理信息。

## 文件

- `kernel/src/drivers/acpi/mod.rs` - ACPI 主模块
- `kernel/src/drivers/acpi/tables.rs` - 表解析
- `kernel/src/drivers/acpi/madt.rs` - MADT 解析
- `kernel/src/drivers/acpi/dmar.rs` - DMAR 解析
- `kernel/src/drivers/acpi/srat.rs` - SRAT 解析

## ACPI 表结构

```
RSDP (Root System Description Pointer)
    │
    ├─> XSDT (Extended System Description Table)
    │       │
    │       ├─> FADT (Fixed ACPI Description Table)
    │       ├─> MADT (Multiple APIC Description Table)
    │       ├─> DMAR (DMA Remapping Table)
    │       └─> SRAT (System Resource Affinity Table)
```

## RSDP 定位

```rust
pub fn init(rsdp_addr: u64) -> Result<(), AcpiError>
```

**查找 RSDP**:
1. UEFI 配置表查找
2. 传统 BIOS 区域查找 (0xE0000 - 0xFFFFF)

**RSDP 结构**:
```rust
#[repr(C, packed)]
pub struct Rsdp {
    pub signature: [u8; 8],    // "RSD PTR "
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,
    // ACPI 2.0+
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
}
```

## MADT 解析

**MADT 结构**:
```rust
#[repr(C, packed)]
pub struct Madt {
    pub header: SdtHeader,
    pub local_apic_address: u32,
    pub flags: u32,
    // Entries follow...
}
```

**MADT 条目类型**:
| 类型 | 名称 | 说明 |
|------|------|------|
| 0 | Processor Local APIC | CPU APIC |
| 1 | I/O APIC | I/O APIC |
| 2 | Interrupt Source Override | IRQ 覆盖 |
| 4 | NMI Source | NMI 源 |

**解析 MADT**:
```rust
pub fn parse_madt(madt: &Madt) -> MadtInfo
```

```rust
pub struct MadtInfo {
    pub local_apic_address: u64,
    pub cpu_count: usize,
    pub ioapics: Vec<IoApicEntry>,
    pub iso_overrides: Vec<IsoOverrideEntry>,
    pub nmi_sources: Vec<NmiEntry>,
}
```

## DMAR 解析

**DMAR 结构**:
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

**DRHD 单元**:
```rust
pub struct DrhdUnit {
    pub register_address: u64,
    pub segment_number: u16,
    pub devices: Vec<DmarScope>,
}
```

**解析 DMAR**:
```rust
pub fn parse_dmar(dmar: &Dmar) -> DmarInfo
```

## SRAT 解析

**SRAT 结构**:
```rust
#[repr(C, packed)]
pub struct Srat {
    pub header: SdtHeader,
    // Affinity structures follow...
}
```

**内存亲和性**:
```rust
pub struct MemoryAffinity {
    pub proximity_domain: u32,
    pub base_address: u64,
    pub end_address: u64,
    pub enabled: bool,
}
```

**解析 SRAT**:
```rust
pub fn parse_srat(srat: &Srat) -> SratInfo
```

## 电源管理

### 关机

```rust
pub fn acpi_shutdown() -> Result<(), &'static str>
pub fn get_shutdown_info() -> Option<(u16, u16)>
```

**FADT 表**:
```rust
pub struct Fadt {
    pub pm1a_cnt_blk: u32,
    pub pm1b_cnt_blk: u32,
    // ...
}
```

**关机流程**:
```rust
// 获取 PM1 寄存器
let (pm1a_cnt, pm1b_cnt) = get_shutdown_info()?;

// 写入 SLP_TYP 值 (5 = Soft Off)
unsafe {
    if pm1a_cnt != 0 {
        let port = pm1a_cnt as u16;
        core::arch::asm!("out dx, ax", in("dx") port, in("ax") 0x2400u16);
    }
    if pm1b_cnt != 0 {
        let port = pm1b_cnt as u16;
        core::arch::asm!("out dx, ax", in("dx") port, in("ax") 0x2400u16);
    }
}
```

### 重启

```rust
pub fn acpi_reset() -> Result<(), &'static str>
```

**复位寄存器**:
```rust
// FADT 中的复位寄存器
let reset_reg = fadt.reset_reg.unwrap() as u16;
let reset_value = fadt.reset_value.unwrap();

// 写入复位值
unsafe {
    core::arch::asm!("out dx, ax", in("dx") reset_reg, in("ax") reset_value);
}
```

## 使用示例

### 初始化 ACPI

```rust
use kernel::drivers::acpi::{init, get_table, MADT_SIGNATURE, DMAR_SIGNATURE, SRAT_SIGNATURE};

// 从 BootInfo 获取 RSDP 地址
let rsdp_addr = boot_info.acpi_rsdp_addr;

if rsdp_addr != 0 {
    init(rsdp_addr)?;
}
```

### 解析 MADT

```rust
use kernel::drivers::acpi::{get_table, MADT_SIGNATURE, parse_madt};

if let Some(madt) = get_table::<Madt>(MADT_SIGNATURE) {
    let info = parse_madt(madt);

    kprintln!("CPUs: {}", info.cpu_count);
    kprintln!("Local APIC: {:#x}", info.local_apic_address);

    for ioapic in &info.ioapics {
        kprintln!("I/O APIC: {:#x}", ioapic.address);
    }
}
```

### 解析 DMAR (IOMMU)

```rust
use kernel::drivers::acpi::{get_table, DMAR_SIGNATURE, parse_dmar};

if let Some(dmar) = get_table::<Dmar>(DMAR_SIGNATURE) {
    let info = parse_dmar(dmar);

    kprintln!("IOMMU units: {}", info.drhd_units.len());

    for drhd in &info.drhd_units {
        kprintln!("  Unit: {:#x}", drhd.register_address);
    }
}
```

### 解析 SRAT (NUMA)

```rust
use kernel::drivers::acpi::{get_table, SRAT_SIGNATURE, parse_srat};
use kernel::mm::numa::init_numa;

if let Some(srat) = get_table::<Srat>(SRAT_SIGNATURE) {
    let info = parse_srat(srat);

    // 初始化 NUMA
    init_numa(&info)?;

    for affinity in &info.memory_affinity {
        kprintln!("Node {}: {:#x} - {:#x}",
            affinity.proximity_domain,
            affinity.base_address,
            affinity.end_address);
    }
}
```

## 相关文档

- [API: acpi](../api/drivers/acpi.md)
- [IOMMU](./iommu.md)
- [APIC](./apic.md)
- [NUMA](../api/mm/numa.md)
