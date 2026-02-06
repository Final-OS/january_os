//! PCI 核心逻辑
//!
//! 提供 PCI 配置空间访问和设备枚举功能

use core::arch::asm;
use crate::{warn, debug};
use super::pcie;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PciHeader {
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub cache_line_size: u8,
    pub latency_timer: u8,
    pub header_type: u8,
    pub bist: u8,
}

unsafe fn outl(port: u16, val: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

unsafe fn inl(port: u16) -> u32 {
    let ret: u32;
    asm!("in eax, dx", out("eax") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

pub unsafe fn read_config_32(addr: PciAddress, offset: u8) -> u32 {
    // Try PCIe ECAM first if available
    if let Some(val) = pcie::read_config(addr.bus, addr.device, addr.function, offset as u16) {
        return val;
    }

    // Fallback to legacy Port I/O
    let address = 0x80000000 
        | ((addr.bus as u32) << 16) 
        | ((addr.device as u32) << 11) 
        | ((addr.function as u32) << 8) 
        | ((offset as u32) & 0xFC);

    outl(PCI_CONFIG_ADDRESS, address);
    inl(PCI_CONFIG_DATA)
}

pub unsafe fn write_config_32(addr: PciAddress, offset: u8, val: u32) {
    // Try PCIe ECAM first if available
    if pcie::write_config(addr.bus, addr.device, addr.function, offset as u16, val) {
        return;
    }

    // Fallback to legacy Port I/O
    let address = 0x80000000 
        | ((addr.bus as u32) << 16) 
        | ((addr.device as u32) << 11) 
        | ((addr.function as u32) << 8) 
        | ((offset as u32) & 0xFC);

    outl(PCI_CONFIG_ADDRESS, address);
    outl(PCI_CONFIG_DATA, val);
}

pub fn read_header(addr: PciAddress) -> PciHeader {
    let r0 = unsafe { read_config_32(addr, 0x00) };
    let r1 = unsafe { read_config_32(addr, 0x04) };
    let r2 = unsafe { read_config_32(addr, 0x08) };
    let r3 = unsafe { read_config_32(addr, 0x0C) };

    PciHeader {
        vendor_id: (r0 & 0xFFFF) as u16,
        device_id: (r0 >> 16) as u16,
        command: (r1 & 0xFFFF) as u16,
        status: (r1 >> 16) as u16,
        revision_id: (r2 & 0xFF) as u8,
        prog_if: ((r2 >> 8) & 0xFF) as u8,
        subclass: ((r2 >> 16) & 0xFF) as u8,
        class_code: (r2 >> 24) as u8,
        cache_line_size: (r3 & 0xFF) as u8,
        latency_timer: ((r3 >> 8) & 0xFF) as u8,
        header_type: ((r3 >> 16) & 0xFF) as u8,
        bist: (r3 >> 24) as u8,
    }
}

/// 查找 PCI Capability
pub fn find_capability(addr: PciAddress, cap_id: u8) -> Option<u8> {
    let header = read_header(addr);
    // Status Register bit 4 indicates Capabilities List
    if header.status & (1 << 4) == 0 {
        return None;
    }
    
    // Capabilities Pointer is at 0x34
    let mut offset = unsafe { read_config_32(addr, 0x34) } as u8;
    // Bottom 2 bits are reserved, mask them out just in case, though usually 0 for DWORD alignment
    offset &= !0x3;
    
    while offset != 0 {
        let val = unsafe { read_config_32(addr, offset) };
        let id = (val & 0xFF) as u8;
        let next = ((val >> 8) & 0xFF) as u8;
        
        if id == cap_id {
            return Some(offset);
        }
        
        offset = next;
    }
    
    None
}

/// 启用 MSI 中断 (32-bit Address only for now)
pub unsafe fn enable_msi(addr: PciAddress, vector: u8) -> bool {
    if let Some(offset) = find_capability(addr, 0x05) {
        // MSI Capability Found
        let ctrl = read_config_32(addr, offset);
        
        // Address: 0xFEE00000 | (0 << 12) | (0 << 2)
        // Destination ID = 0 (BSP), No Redirection
        let msi_addr = 0xFEE00000;
        let msi_data = vector as u32;
        
        write_config_32(addr, offset + 4, msi_addr);
        
        // Check if 64-bit capable
        let is_64bit = (ctrl & (1 << 23)) != 0;
        let data_offset = if is_64bit { 
            write_config_32(addr, offset + 8, 0); // Upper Address = 0
            0xC 
        } else { 
            0x8 
        };
        
        write_config_32(addr, offset + data_offset, msi_data);
        
        // Enable MSI
        write_config_32(addr, offset, ctrl | (1 << 16));
        
        return true;
    }
    false
}

/// 扫描 PCI 总线
pub fn scan_bus<F>(callback: &mut F)
where
    F: FnMut(PciAddress, PciHeader),
{
    // 扫描总线 0 (通常所有设备都在这，或者通过桥接到其他总线)
    // 简单的实现：扫描 0-255 总线
    for bus in 0..=255 {
        for device in 0..32 {
            check_device(bus, device, callback);
        }
    }
}

fn check_device<F>(bus: u8, device: u8, callback: &mut F)
where
    F: FnMut(PciAddress, PciHeader),
{
    let addr = PciAddress { bus, device, function: 0 };
    // 读取 Vendor ID
    let vendor_id = unsafe { read_config_32(addr, 0) } as u16;
    
    if vendor_id == 0xFFFF {
        return;
    }
    
    let header = read_header(addr);
    callback(addr, header);
    
    // 检查是否为多功能设备 (Header Type Bit 7)
    if (header.header_type & 0x80) != 0 {
        for function in 1..8 {
            let addr = PciAddress { bus, device, function };
            let vendor_id = unsafe { read_config_32(addr, 0) } as u16;
            if vendor_id != 0xFFFF {
                let header = read_header(addr);
                callback(addr, header);
            }
        }
    }
}
