//! PCI 驱动
//!
//! 提供 PCI 配置空间访问和设备枚举功能

use core::arch::asm;

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
    let address = 0x80000000 
        | ((addr.bus as u32) << 16) 
        | ((addr.device as u32) << 11) 
        | ((addr.function as u32) << 8) 
        | ((offset as u32) & 0xFC);

    outl(PCI_CONFIG_ADDRESS, address);
    inl(PCI_CONFIG_DATA)
}

pub unsafe fn write_config_32(addr: PciAddress, offset: u8, val: u32) {
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

pub fn scan_bus<F>(callback: &mut F)
where
    F: FnMut(PciAddress, PciHeader),
{
    for bus in 0..=255 {
        for device in 0..32 {
            let addr = PciAddress { bus, device, function: 0 };
            let vendor_id = unsafe { read_config_32(addr, 0) } & 0xFFFF;
            if vendor_id != 0xFFFF {
                let header = read_header(addr);
                callback(addr, header);
                
                if header.header_type & 0x80 != 0 {
                    for function in 1..8 {
                        let addr = PciAddress { bus, device, function };
                        let vendor_id = unsafe { read_config_32(addr, 0) } & 0xFFFF;
                        if vendor_id != 0xFFFF {
                            let header = read_header(addr);
                            callback(addr, header);
                        }
                    }
                }
            }
        }
    }
}
