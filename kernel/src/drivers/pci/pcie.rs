use crate::drivers::acpi::{AcpiTable, SdtHeader};
use crate::{info, warn};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct McfgAllocation {
    pub base_addr: u64,
    pub pci_seg_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
    pub reserved: u32,
}

#[repr(C, packed)]
pub struct Mcfg {
    pub header: SdtHeader,
    pub reserved: u64,
}

impl AcpiTable for Mcfg {
    fn signature() -> &'static [u8; 4] {
        b"MCFG"
    }
}

impl Mcfg {
    pub fn allocations(&self) -> &[McfgAllocation] {
        let ptr = unsafe { (self as *const Mcfg).add(1) as *const McfgAllocation };
        let len = (self.header.length as usize - core::mem::size_of::<Mcfg>()) / core::mem::size_of::<McfgAllocation>();
        unsafe { core::slice::from_raw_parts(ptr, len) }
    }
}

static mut ECAM_BASE: u64 = 0;

pub fn init() {
    if let Some(mcfg) = crate::drivers::acpi::find_table::<Mcfg>() {
        info!("PCIe: MCFG table found");
        for alloc in mcfg.allocations() {
             let base = alloc.base_addr;
             let start = alloc.start_bus;
             let end = alloc.end_bus;
             let segment = alloc.pci_seg_group;
             
             info!("PCIe: ECAM Base {:#x}, Bus {}-{}", base, start, end);
             // For simplicity, we just take the first one (segment 0) for now
             if segment == 0 && start == 0 {
                 unsafe { ECAM_BASE = base };
             }
        }
    } else {
        warn!("PCIe: MCFG table not found");
    }
}

pub unsafe fn read_config(bus: u8, dev: u8, func: u8, offset: u16) -> Option<u32> {
    if ECAM_BASE == 0 { return None; }
    // Calculate address
    // Address = Base + (Bus << 20) + (Device << 15) + (Function << 12) + Offset
    let addr = ECAM_BASE + ((bus as u64) << 20) + ((dev as u64) << 15) + ((func as u64) << 12) + (offset as u64);
    
    // Map physical to virtual
    let virt_addr = addr + crate::config::DIRECT_MAP_OFFSET;
    
    Some(core::ptr::read_volatile(virt_addr as *const u32))
}

pub unsafe fn write_config(bus: u8, dev: u8, func: u8, offset: u16, value: u32) -> bool {
    if ECAM_BASE == 0 { return false; }
    
    let addr = ECAM_BASE + ((bus as u64) << 20) + ((dev as u64) << 15) + ((func as u64) << 12) + (offset as u64);
    let virt_addr = addr + crate::config::DIRECT_MAP_OFFSET;
    
    core::ptr::write_volatile(virt_addr as *mut u32, value);
    true
}
