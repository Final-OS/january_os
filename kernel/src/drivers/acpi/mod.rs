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
    if ACPI_STATE.is_initialized() {
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

    // 保存状态
    ACPI_STATE.set(AcpiState {
        xsdt_addr: sdt_addr,
        revision: rsdp.revision,
    });

    Ok(())
}

/// 打印所有 ACPI 表
pub fn dump_tables() {
    let state = match ACPI_STATE.get() {
        Some(s) => s,
        None => {
            crate::kprintln!("ACPI not initialized");
            return;
        }
    };
    let xsdt_addr = state.xsdt_addr;

    crate::kprintln!("ACPI Revision: {}", state.revision);
    crate::kprintln!("XSDT/RSDT Address: {:#x}", state.xsdt_addr);

    unsafe {
        let xsdt_virt = phys_to_virt(xsdt_addr);
        let header = &*(xsdt_virt as *const SdtHeader);

        // Copy fields to avoid unaligned access
        let revision = header.revision;
        let length = header.length;
        
        let is_xsdt = &header.signature == b"XSDT";
        
        if is_xsdt {
            crate::kprintln!("XSDT (v{}) Length: {}", revision, length);
        } else {
            crate::kprintln!("RSDT (v{}) Length: {}", revision, length);
        }

        let entry_size = if is_xsdt { 8 } else { 4 };
        let entries_size = length as usize - core::mem::size_of::<SdtHeader>();
        let entry_count = entries_size / entry_size;

        let entries_start = xsdt_virt + core::mem::size_of::<SdtHeader>() as u64;
        
        for i in 0..entry_count {
            let entry_addr = if is_xsdt {
                let ptr = (entries_start + (i * 8) as u64) as *const u64;
                *ptr
            } else {
                let ptr = (entries_start + (i * 4) as u64) as *const u32;
                *ptr as u64
            };

            if entry_addr == 0 {
                continue;
            }

            let table_virt = phys_to_virt(entry_addr);
            let table_header = &*(table_virt as *const SdtHeader);
            
            // Copy fields to avoid unaligned access
            let sig = table_header.signature;
            let len = table_header.length;
            let sig_str = core::str::from_utf8(&sig).unwrap_or("????");
            
            crate::kprintln!("  [{}] {} @ {:#x} (len: {})", 
                i, sig_str, entry_addr, len);
        }
    }
}

/// 查找 ACPI 表
pub fn find_table<T: AcpiTable>() -> Option<&'static T> {
    let state = ACPI_STATE.get()?;
    let xsdt_addr = state.xsdt_addr;

    unsafe {
        let xsdt_virt = phys_to_virt(xsdt_addr);
        let header = &*(xsdt_virt as *const SdtHeader);

        // 验证 XSDT
        if &header.signature != b"XSDT" && &header.signature != b"RSDT" {
            return None;
        }

        // 计算表条目数量
        let is_xsdt = &header.signature == b"XSDT";
        let entry_size = if is_xsdt { 8 } else { 4 };
        let entries_size = header.length as usize - core::mem::size_of::<SdtHeader>();
        let entry_count = entries_size / entry_size;

        // 遍历所有条目
        let entries_start = xsdt_virt + core::mem::size_of::<SdtHeader>() as u64;
        
        for i in 0..entry_count {
            let entry_addr = if is_xsdt {
                let ptr = (entries_start + (i * 8) as u64) as *const u64;
                *ptr
            } else {
                let ptr = (entries_start + (i * 4) as u64) as *const u32;
                *ptr as u64
            };

            if entry_addr == 0 {
                continue;
            }

            // 读取表头并检查签名
            let table_virt = phys_to_virt(entry_addr);
            let table_header = &*(table_virt as *const SdtHeader);

            if &table_header.signature == T::signature() {
                return Some(&*(table_virt as *const T));
            }
        }
    }

    None
}

// Removed get_table
/// ACPI 表 trait
pub trait AcpiTable {
    /// 获取表签名
    fn signature() -> &'static [u8; 4];
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 物理地址转虚拟地址
#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    phys + crate::config::DIRECT_MAP_OFFSET
}

/// 验证 ACPI 表校验和
fn validate_checksum(data: *const u8, length: usize) -> bool {
    let mut sum: u8 = 0;
    for i in 0..length {
        sum = sum.wrapping_add(unsafe { *data.add(i) });
    }
    sum == 0
}

// ============================================================================
// 调试
// ============================================================================

// Removed duplicate dump_tables
// ============================================================================
// FADT (Fixed ACPI Description Table) - 电源管理
// ============================================================================

/// FADT 结构（简化版，只包含关机需要的字段）
#[repr(C, packed)]
pub struct Fadt {
    pub header: SdtHeader,
    pub firmware_ctrl: u32,
    pub dsdt: u32,
    pub _reserved1: u8,
    pub preferred_pm_profile: u8,
    pub sci_int: u16,
    pub smi_cmd: u32,
    pub acpi_enable: u8,
    pub acpi_disable: u8,
    pub s4bios_req: u8,
    pub pstate_cnt: u8,
    pub pm1a_evt_blk: u32,
    pub pm1b_evt_blk: u32,
    pub pm1a_cnt_blk: u32,      // PM1a Control Block - 关机用
    pub pm1b_cnt_blk: u32,      // PM1b Control Block
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
    pub day_alrm: u8,
    pub mon_alrm: u8,
    pub century: u8,
    pub iapc_boot_arch: u16,
    pub _reserved2: u8,
    pub flags: u32,
    // ... 更多字段省略
}


impl AcpiTable for Fadt {
    fn signature() -> &'static [u8; 4] {
        b"FACP"
    }
}

/// 获取 FADT 表
pub fn get_fadt() -> Option<&'static Fadt> {
    find_table::<Fadt>()
}

/// 获取关机信息（用于调试）
pub fn get_shutdown_info() -> Option<(u16, u16)> {
    let fadt = get_fadt()?;
    Some((fadt.pm1a_cnt_blk as u16, fadt.pm1b_cnt_blk as u16))
}

/// ACPI 关机
/// 
/// 从 FADT 读取正确的 PM1a_CNT 端口并执行 S5 睡眠
pub fn acpi_shutdown() -> Result<(), &'static str> {
    // 获取 FADT
    let fadt = get_fadt().ok_or("FADT not found")?;
    
    let pm1a_cnt = fadt.pm1a_cnt_blk as u16;
    if pm1a_cnt == 0 {
        return Err("PM1a_CNT_BLK is 0");
    }
    
    let pm1b_cnt = fadt.pm1b_cnt_blk as u16;
    
    // SLP_EN 是 bit 13
    // SLP_TYPa 在 bit 10-12，尝试所有可能值 0-7
    unsafe {
        for slp_typ in [5u16, 7, 0, 1, 2, 3, 4, 6] {
            let slp_value: u16 = (slp_typ << 10) | (1 << 13);
            core::arch::asm!("out dx, ax", in("dx") pm1a_cnt, in("ax") slp_value);
            
            if pm1b_cnt != 0 {
                core::arch::asm!("out dx, ax", in("dx") pm1b_cnt, in("ax") slp_value);
            }
            
            // 短暂延迟
            for _ in 0..10000 {
                core::arch::asm!("pause");
            }
        }
    }
    
    Ok(())
}
