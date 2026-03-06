//! MSI-X 中断支持
//!
//! 提供 MSI-X 的配置和启用功能

use crate::drivers::pci::{find_capability, read_config_32, write_config_32, PciAddress};
use crate::warn;

/// 启用 MSI-X 中断
///
/// # Arguments
/// * `addr` - PCI 设备地址
/// * `table_base` - MSI-X 表所在的 BAR 的虚拟基地址 (目前只支持 BIR=0)
/// * `vector` - 中断向量号
pub unsafe fn enable_msix(addr: PciAddress, table_base: *mut u8, vector: u8) -> bool {
    if let Some(offset) = find_capability(addr, 0x11) {
        // Found MSI-X Capability
        // Read Table Offset and BIR (Bar Indicator Register)
        let table_reg = read_config_32(addr, offset + 4);
        let bir = (table_reg & 0x7) as u8;
        let table_offset = (table_reg & !0x7) as usize;

        if bir != 0 {
            warn!(
                "[PCI] MSI-X Table in BAR {}, not supported (only BAR0 supported with provided base)",
                bir
            );
            return false;
        }

        // Write Entry 0
        // Structure: MsgAddr(L), MsgAddr(H), MsgData, VectorControl
        let table_entry_addr = table_base.add(table_offset) as *mut u32;

        // Message Address: 0xFEE00000 (Local APIC)
        core::ptr::write_volatile(table_entry_addr, 0xFEE00000);
        core::ptr::write_volatile(table_entry_addr.add(1), 0);

        // Message Data: Vector
        core::ptr::write_volatile(table_entry_addr.add(2), vector as u32);

        // Vector Control: Unmasked (0)
        core::ptr::write_volatile(table_entry_addr.add(3), 0);

        // Enable MSI-X Global Mask
        let control_reg = read_config_32(addr, offset);
        let mut control = (control_reg >> 16) as u16;
        control |= 0x8000; // Enable
        control &= !(1 << 14); // Function Mask (Unmask all)

        write_config_32(
            addr,
            offset,
            (control_reg & 0xFFFF) | ((control as u32) << 16),
        );

        return true;
    }
    false
}
