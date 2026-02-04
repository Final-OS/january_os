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

    // 设置状态（只会执行一次）
    ACPI_STATE.set(AcpiState {
        xsdt_addr: sdt_addr,
        revision: rsdp.revision,
    }).map_err(|_| "ACPI already initialized")?;

    Ok(())
}

/// 检查 ACPI 是否已初始化
pub fn initialized() -> bool {
    ACPI_STATE.is_initialized()
}

/// 获取 ACPI 版本
pub fn revision() -> u8 {
    ACPI_STATE.get().map(|s| s.revision).unwrap_or(0)
}

// ============================================================================
// 表查找
// ============================================================================

/// 查找 ACPI 表
///
/// # Arguments
/// * `signature` - 表签名（4 字节 ASCII）
///
/// # Returns
/// 成功返回表的物理地址，未找到返回 None
pub fn find_table(signature: &[u8; 4]) -> Option<u64> {
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

            if &table_header.signature == signature {
                return Some(entry_addr);
            }
        }
    }

    None
}

/// 获取表并验证
pub fn get_table<T: AcpiTable>(signature: &[u8; 4]) -> Option<&'static T> {
    let phys = find_table(signature)?;
    let virt = phys_to_virt(phys);
    
    unsafe {
        let header = &*(virt as *const SdtHeader);
        
        // 验证校验和
        if !validate_checksum(virt as *const u8, header.length as usize) {
            return None;
        }
        
        Some(&*(virt as *const T))
    }
}

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

/// 打印所有 ACPI 表
pub fn dump_tables() {
    let state = match ACPI_STATE.get() {
        Some(s) => s,
        None => return,
    };
    let xsdt_addr = state.xsdt_addr;

    unsafe {
        let xsdt_virt = phys_to_virt(xsdt_addr);
        let header = &*(xsdt_virt as *const SdtHeader);

        let is_xsdt = &header.signature == b"XSDT";
        let entry_size = if is_xsdt { 8 } else { 4 };
        let entries_size = header.length as usize - core::mem::size_of::<SdtHeader>();
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
            
            // 签名转字符串
            let sig = core::str::from_utf8(&table_header.signature).unwrap_or("????");
            let _ = sig; // 由调用者打印
        }
    }
}

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

/// 获取 FADT 表
pub fn get_fadt() -> Option<&'static Fadt> {
    let phys = find_table(b"FACP")?;  // FADT 签名是 "FACP"
    let virt = phys_to_virt(phys);
    unsafe { Some(&*(virt as *const Fadt)) }
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
