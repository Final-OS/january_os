use super::tables::{SdtHeader, AcpiTable};
use core::mem;

/// MADT (Multiple APIC Description Table)
///
/// Contains interrupt controller info (Local APIC, IO APIC, Overrides).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Madt {
    pub header: SdtHeader,
    pub local_apic_addr: u32,
    pub flags: u32,
}

impl AcpiTable for Madt {
    fn signature() -> &'static [u8; 4] {
        b"APIC"
    }
}

impl Madt {
    /// Check if system has 8259 PICs (PC-AT Compatibility)
    pub fn has_8259_pic(&self) -> bool {
        (self.flags & 1) != 0
    }

    /// Get Local APIC Address (Physical)
    pub fn local_apic_addr(&self) -> u64 {
        self.local_apic_addr as u64
    }

    /// Iterator over MADT entries
    pub fn entries(&self) -> MadtEntryIter {
        let start = (self as *const _ as usize + mem::size_of::<Madt>()) as *const u8;
        let end = (self as *const _ as usize + self.header.length as usize) as *const u8;
        MadtEntryIter { current: start, end }
    }
    
    pub fn get_multiprocessor_wakeup(&self) -> Option<&MultiprocessorWakeup> {
        for entry in self.entries() {
            if let MadtEntry::MultiprocessorWakeup(w) = entry {
                return Some(w);
            }
        }
        None
    }
}

/// Iterator for MADT entries
pub struct MadtEntryIter {
    current: *const u8,
    end: *const u8,
}

impl Iterator for MadtEntryIter {
    type Item = MadtEntry<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }
        
        unsafe {
            let entry_type = *self.current;
            let entry_len = *self.current.add(1);
            
            if entry_len == 0 {
                return None; // Prevent infinite loop on bad data
            }
            
            let entry = match entry_type {
                0 => MadtEntry::LocalApic(&*(self.current as *const LocalApic)),
                1 => MadtEntry::IoApic(&*(self.current as *const IoApic)),
                2 => MadtEntry::InterruptSourceOverride(&*(self.current as *const InterruptSourceOverride)),
                4 => MadtEntry::Nmi(&*(self.current as *const Nmi)),
                16 => MadtEntry::MultiprocessorWakeup(&*(self.current as *const MultiprocessorWakeup)),
                _ => MadtEntry::Unknown(entry_type),
            };
            
            self.current = self.current.add(entry_len as usize);
            Some(entry)
        }
    }
}

/// MADT Entry Enum
#[derive(Debug)]
pub enum MadtEntry<'a> {
    LocalApic(&'a LocalApic),
    IoApic(&'a IoApic),
    InterruptSourceOverride(&'a InterruptSourceOverride),
    Nmi(&'a Nmi),
    MultiprocessorWakeup(&'a MultiprocessorWakeup),
    Unknown(u8),
}

// === Entry Structures ===

/// Type 0: Processor Local APIC
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApic {
    pub entry_type: u8,
    pub length: u8,
    pub acpi_processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

impl LocalApic {
    pub fn is_enabled(&self) -> bool {
        (self.flags & 1) != 0
    }
    pub fn is_online_capable(&self) -> bool {
        (self.flags & 2) != 0
    }
}

/// Type 1: I/O APIC
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IoApic {
    pub entry_type: u8,
    pub length: u8,
    pub io_apic_id: u8,
    pub reserved: u8,
    pub io_apic_address: u32,
    pub global_system_interrupt_base: u32,
}

/// Type 2: Interrupt Source Override
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptSourceOverride {
    pub entry_type: u8,
    pub length: u8,
    pub bus: u8,
    pub source: u8,
    pub global_system_interrupt: u32,
    pub flags: u16,
}

/// Type 4: Local APIC NMI
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Nmi {
    pub entry_type: u8,
    pub length: u8,
    pub acpi_processor_id: u8,
    pub flags: u16,
    pub lint: u8,
}

/// Type 16: Multiprocessor Wakeup Structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MultiprocessorWakeup {
    pub entry_type: u8,
    pub length: u8,
    pub mailbox_version: u16,
    pub reserved: u32,
    pub mailbox_address: u64,
}

/// Multiprocessor Wakeup Mailbox
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MultiprocessorWakeupMailbox {
    pub command: u16,
    pub reserved: u16,
    pub apic_id: u32,
    pub wakeup_vector: u64,
    pub reserved_os: [u8; 2032],
}

impl MultiprocessorWakeupMailbox {
    pub const COMMAND_NOOP: u16 = 0;
    pub const COMMAND_WAKEUP: u16 = 1;
}

// ============================================================================
// Helper Structs & Parsing
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct IoApicInfo {
    pub id: u8,
    pub address: u32,
    pub gsi_base: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct IrqOverrideInfo {
    pub source: u8,
    pub gsi: u32,
    pub level_triggered: bool,
    pub active_low: bool,
}

#[derive(Debug, Clone)]
pub struct MadtInfo {
    pub cpu_count: usize,
    pub local_apic_address: u64,
    pub ioapic_count: usize,
    pub ioapics: [IoApicInfo; 16], // Limit to 16 IOAPICs
    pub irq_override_count: usize,
    pub irq_overrides: [IrqOverrideInfo; 16], // Limit to 16 ISA IRQ overrides
}

pub fn parse_madt(madt: &Madt) -> MadtInfo {
    let mut info = MadtInfo {
        cpu_count: 0,
        local_apic_address: madt.local_apic_addr(),
        ioapic_count: 0,
        ioapics: [IoApicInfo { id: 0, address: 0, gsi_base: 0 }; 16],
        irq_override_count: 0,
        irq_overrides: [IrqOverrideInfo {
            source: 0,
            gsi: 0,
            level_triggered: false,
            active_low: false,
        }; 16],
    };

    for entry in madt.entries() {
        match entry {
            MadtEntry::LocalApic(lapic) => {
                if lapic.is_enabled() || lapic.is_online_capable() {
                    info.cpu_count += 1;
                }
            },
            MadtEntry::IoApic(ioapic) => {
                if info.ioapic_count < 16 {
                    info.ioapics[info.ioapic_count] = IoApicInfo {
                        id: ioapic.io_apic_id,
                        address: ioapic.io_apic_address,
                        gsi_base: ioapic.global_system_interrupt_base,
                    };
                    info.ioapic_count += 1;
                }
            },
            MadtEntry::InterruptSourceOverride(iso) => {
                if info.irq_override_count < 16 {
                    // 仅处理 ISA 总线 override（bus=0）。
                    if iso.bus != 0 {
                        continue;
                    }

                    let flags = iso.flags;
                    let polarity = flags & 0b11;
                    let trigger = (flags >> 2) & 0b11;

                    // ISA 默认：edge + active high.
                    // ACPI flags:
                    // polarity: 0=conform, 1=high, 3=low
                    // trigger : 0=conform, 1=edge, 3=level
                    let active_low = matches!(polarity, 0b11);
                    let level_triggered = matches!(trigger, 0b11);

                    info.irq_overrides[info.irq_override_count] = IrqOverrideInfo {
                        source: iso.source,
                        gsi: iso.global_system_interrupt,
                        level_triggered,
                        active_low,
                    };
                    info.irq_override_count += 1;
                }
            }
            _ => {}
        }
    }
    
    // Ensure at least 1 CPU
    if info.cpu_count == 0 {
        info.cpu_count = 1;
    }
    
    info
}
