// ============================================================================
// january_os - ACPI (Advanced Configuration and Power Interface)
//
// ACPI 表解析模块，用于获取系统硬件配置信息
// ============================================================================
//!
//! # ACPI 概述
//!
//! ACPI 提供了操作系统与固件之间的标准接口，包含：
//! - 硬件配置信息（CPU、中断控制器、IOMMU 等）
//! - 电源管理接口
//! - 热插拔支持
//!
//! # 主要表格
//!
//! ```text
//! RSDP (Root System Description Pointer)
//!   └── XSDT (Extended System Description Table)
//!         ├── MADT - CPU 和中断控制器信息
//!         ├── DMAR - Intel VT-d IOMMU 信息
//!         ├── IVRS - AMD-Vi IOMMU 信息
//!         ├── SRAT - NUMA 内存亲和性信息
//!         ├── SLIT - NUMA 距离矩阵
//!         ├── FADT - 电源管理信息
//!         └── ...
//! ```
//!
//! # 使用方法
//!
//! ```rust,ignore
//! // 初始化 ACPI（需要 UEFI 提供的 RSDP 地址）
//! acpi::init(rsdp_addr)?;
//!
//! // 查找特定表
//! if let Some(madt) = acpi::find_table::<Madt>() {
//!     // 解析 MADT
//! }
//! ```

mod tables;
mod madt;
mod dmar;
mod srat;

pub use tables::*;
pub use madt::*;
pub use dmar::*;
pub use srat::*;

use crate::sync::OnceCell;

// ============================================================================
// 全局状态
// ============================================================================

/// ACPI 状态
struct AcpiState {
    /// XSDT/RSDT 物理地址
    xsdt_addr: u64,
    /// ACPI 版本 (1 = ACPI 1.0, 2+ = ACPI 2.0+)
    revision: u8,
}

/// ACPI 全局状态（一次性初始化）
static ACPI_STATE: OnceCell<AcpiState> = OnceCell::new();

/// 检查 ACPI 是否已初始化
pub fn is_initialized() -> bool {
    ACPI_STATE.get().is_some()
}

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 ACPI 子系统
///
/// # Arguments
/// * `rsdp_addr` - RSDP 物理地址（从 UEFI 获取）
///
/// # Returns
/// 成功返回 Ok(()), 失败返回错误信息
pub fn init(rsdp_addr: u64) -> Result<(), &'static str> {
    if ACPI_STATE.get().is_some() {
        return Err("ACPI already initialized");
    }

    if rsdp_addr == 0 {
        return Err("Invalid RSDP address");
    }

    // 验证并解析 RSDP
    let rsdp = unsafe { Rsdp::from_addr(rsdp_addr)? };

    // 获取 XSDT/RSDT 地址
    let sdt_addr = if rsdp.revision >= 2 {
        // ACPI 2.0+: 使用 XSDT
        rsdp.xsdt_address
    } else {
        // ACPI 1.0: 使用 RSDT
        rsdp.rsdt_address as u64
    };

    if sdt_addr == 0 {
        return Err("Invalid XSDT/RSDT address");
    }

    // 使用 set 初始化 OnceCell
    let _ = ACPI_STATE.set(AcpiState {
        xsdt_addr: sdt_addr,
        revision: rsdp.revision,
    });

    Ok(())
}

// ============================================================================
// 表查找与遍历
// ============================================================================

/// 查找特定的 ACPI 表
pub fn find_table<T: AcpiTable>() -> Option<&'static T> {
    let state = ACPI_STATE.get()?;
    let virt_addr = state.xsdt_addr + crate::config::DIRECT_MAP_OFFSET;
    let signature = T::signature();

    if state.revision >= 2 {
        // 使用 XSDT
        let xsdt = unsafe { &*(virt_addr as *const Xsdt) };
        let count = xsdt.entry_count();
        for i in 0..count {
            let entry_addr = unsafe { xsdt.entry(i) };
            if entry_addr == 0 { continue; }
            let entry_virt = entry_addr + crate::config::DIRECT_MAP_OFFSET;
            let header = unsafe { &*(entry_virt as *const SdtHeader) };
            if &header.signature == signature {
                return unsafe { Some(&*(entry_virt as *const T)) };
            }
        }
    } else {
        // 使用 RSDT
        let rsdt = unsafe { &*(virt_addr as *const Rsdt) };
        let count = rsdt.entry_count();
        for i in 0..count {
            let entry_addr = unsafe { rsdt.entry(i) } as u64;
            if entry_addr == 0 { continue; }
            let entry_virt = entry_addr + crate::config::DIRECT_MAP_OFFSET;
            let header = unsafe { &*(entry_virt as *const SdtHeader) };
            if &header.signature == signature {
                return unsafe { Some(&*(entry_virt as *const T)) };
            }
        }
    }

    None
}

/// 打印所有 ACPI 表信息
pub fn dump_tables() {
    let state = if let Some(s) = ACPI_STATE.get() { s } else {
        crate::warn!("ACPI not initialized");
        return;
    };

    let virt_addr = state.xsdt_addr + crate::config::DIRECT_MAP_OFFSET;

    if state.revision >= 2 {
        let xsdt = unsafe { &*(virt_addr as *const Xsdt) };
        crate::info!("ACPI: XSDT at {:#x}, entries: {}", state.xsdt_addr, xsdt.entry_count());
        for i in 0..xsdt.entry_count() {
            let entry_addr = unsafe { xsdt.entry(i) };
            if entry_addr == 0 { continue; }
            let entry_virt = entry_addr + crate::config::DIRECT_MAP_OFFSET;
            let header = unsafe { &*(entry_virt as *const SdtHeader) };
            let signature = header.signature_str();
            let length = header.length;
            let revision = header.revision;
            crate::info!("  [{:02}] {} (len={:<5}, rev={})", 
                i, signature, length, revision);
        }
    } else {
        let rsdt = unsafe { &*(virt_addr as *const Rsdt) };
        crate::info!("ACPI: RSDT at {:#x}, entries: {}", state.xsdt_addr, rsdt.entry_count());
        for i in 0..rsdt.entry_count() {
            let entry_addr = unsafe { rsdt.entry(i) } as u64;
            if entry_addr == 0 { continue; }
            let entry_virt = entry_addr + crate::config::DIRECT_MAP_OFFSET;
            let header = unsafe { &*(entry_virt as *const SdtHeader) };
            let signature = header.signature_str();
            let length = header.length;
            let revision = header.revision;
            crate::info!("  [{:02}] {} (len={:<5}, rev={})", 
                i, signature, length, revision);
        }
    }
}

// ============================================================================
// 电源管理 (FADT)
// ============================================================================

/// FADT (Fixed ACPI Description Table)
#[repr(C, packed)]
pub struct Fadt {
    pub header: SdtHeader,
    pub firmware_ctrl: u32,
    pub dsdt: u32,
    pub reserved: u8,
    pub preferred_pm_profile: u8,
    pub sci_int: u16,
    pub smi_cmd: u32,
    pub acpi_enable: u8,
    pub acpi_disable: u8,
    pub s4bios_req: u8,
    pub pstate_cnt: u8,
    pub pm1a_evt_blk: u32,
    pub pm1b_evt_blk: u32,
    pub pm1a_cnt_blk: u32,
    pub pm1b_cnt_blk: u32,
    pub pm2_cnt_blk: u32,
    pub pm_tmr_blk: u32,
    pub gpe0_blk: u32,
    pub gpe1_blk: u32,
    pub pm1_evt_len: u8,
    pub pm1_cnt_len: u8,
    pub pm2_cnt_len: u8,
    pub pm_tmr_len: u8,
    pub gpe0_blk_len: u8,
    pub gpe1_blk_len: u8,
    pub gpe1_base: u8,
    pub cst_cnt: u8,
    pub p_lvl2_lat: u16,
    pub p_lvl3_lat: u16,
    pub flush_size: u16,
    pub flush_stride: u16,
    pub duty_offset: u8,
    pub duty_width: u8,
    pub day_alarm: u8,
    pub mon_alarm: u8,
    pub century: u8,
    pub iapc_boot_arch: u16,
    pub reserved2: u8,
    pub flags: u32,
    pub reset_reg: GenericAddress,
    pub reset_value: u8,
    pub reserved3: [u8; 3],
    pub x_firmware_ctrl: u64,
    pub x_dsdt: u64,
    pub x_pm1a_evt_blk: GenericAddress,
    pub x_pm1b_evt_blk: GenericAddress,
    pub x_pm1a_cnt_blk: GenericAddress,
    pub x_pm1b_cnt_blk: GenericAddress,
    // ... 更多字段省略 ...
}

impl AcpiTable for Fadt {
    fn signature() -> &'static [u8; 4] { b"FACP" }
}

/// 获取关机所需的 PM1a/PM1b 端口地址
pub fn get_shutdown_info() -> Option<(u32, u32)> {
    if let Some(fadt) = find_table::<Fadt>() {
        // 优先使用 32 位地址 (ACPI 1.0)
        let pm1a = fadt.pm1a_cnt_blk;
        let pm1b = fadt.pm1b_cnt_blk;
        if pm1a != 0 {
            return Some((pm1a, pm1b));
        }
        
        // 尝试使用 64 位地址 (ACPI 2.0+)
        // 注意：这里简化处理，假设是 I/O 端口
        if fadt.x_pm1a_cnt_blk.address_space == 1 { // SystemIo
             return Some((fadt.x_pm1a_cnt_blk.address as u32, fadt.x_pm1b_cnt_blk.address as u32));
        }
    }
    None
}

/// 尝试通过 ACPI 关机
pub fn acpi_shutdown() -> Result<(), &'static str> {
    let (pm1a, pm1b) = get_shutdown_info().ok_or("FADT not found or invalid PM1a_CNT")?;
    
    // SLP_TYP_S5 通常是 5 (101b)
    // SLP_EN 是 bit 13
    // 需要将 SLP_TYP_S5 << 10 | SLP_EN 写入 PM1a_CNT
    
    // 注意：这里的 SLP_TYP 值实际上应该从 DSDT 的 \_S5 包中获取
    // 但对于 QEMU 和大多数标准 PC，SLP_TYP_S5 通常是 0 或 5
    // QEMU 似乎期望 SLP_TYP = 5
    
    let shutdown_val: u16 = (5 << 10) | (1 << 13);
    
    unsafe {
        core::arch::asm!("out dx, ax", in("dx") pm1a as u16, in("ax") shutdown_val);
        if pm1b != 0 {
            core::arch::asm!("out dx, ax", in("dx") pm1b as u16, in("ax") shutdown_val);
        }
    }
    
    // 如果还没关机...
    for _ in 0..1000 { core::hint::spin_loop(); }
    
    Err("ACPI shutdown failed (hardware didn't respond)")
}

/// 尝试通过 ACPI 重启
pub fn acpi_reset() -> Result<(), &'static str> {
    if let Some(fadt) = find_table::<Fadt>() {
        // 检查 FADT 版本 (ACPI 2.0+)
        if fadt.header.revision >= 2 {
            let reset_reg = fadt.reset_reg;
            let reset_val = fadt.reset_value;
            
            // 目前只支持 I/O 端口 (Space ID = 1)
            if reset_reg.address_space == 1 {
                 unsafe {
                     core::arch::asm!("out dx, al", in("dx") reset_reg.address as u16, in("al") reset_val);
                 }
                 // 等待重启
                 for _ in 0..1000 { core::hint::spin_loop(); }
                 return Ok(());
            }
        }
    }
    Err("ACPI reset not supported or failed")
}

// ============================================================================
// 高级配置接口
// ============================================================================

/// 从 ACPI 获取的系统配置摘要
#[derive(Debug, Clone, Copy)]
pub struct AcpiConfig {
    pub cpu_count: usize,
    pub local_apic_addr: u64,
    pub ioapic_addr: u64,
    pub ioapic_gsi_base: u32,
    pub has_iommu: bool,
}

impl Default for AcpiConfig {
    fn default() -> Self {
        Self {
            cpu_count: 1,
            local_apic_addr: 0xFEE00000, // 默认 x86 Local APIC 地址
            ioapic_addr: 0,
            ioapic_gsi_base: 0,
            has_iommu: false,
        }
    }
}

/// 自动检测系统配置
pub fn detect_system_config() -> AcpiConfig {
    let mut config = AcpiConfig::default();

    // 必须先初始化 ACPI
    if ACPI_STATE.get().is_none() {
        return config;
    }

    // 解析 MADT
    if let Some(madt) = find_table::<Madt>() {
        let madt_info = parse_madt(madt);
        config.cpu_count = madt_info.cpu_count;
        config.local_apic_addr = madt_info.local_apic_address;
        
        if madt_info.ioapic_count > 0 {
            config.ioapic_addr = madt_info.ioapics[0].address as u64;
            config.ioapic_gsi_base = madt_info.ioapics[0].gsi_base;
        }
    }

    // 检测 DMAR (IOMMU)
    if find_table::<Dmar>().is_some() {
        config.has_iommu = true;
    }

    config
}
