//! xHCI (Extensible Host Controller Interface) 驱动
//!
//! xHCI 是 USB 3.0+ 的主机控制器标准接口

use crate::drivers::pci::{self, PciAddress, PciHeader};
use crate::{kprintln, kprint, info, ok, warn, error, debug};
use crate::mm::vmalloc::{ioremap, iounmap};
use crate::interrupt::arch::x86_64::idt::IRQ_XHCI;
use crate::mm::buddy::alloc_pages;
use crate::mm::page::page_to_pfn;
use crate::mm::zone::{GfpFlags, GFP_KERNEL_ZERO};
use crate::config::{PAGE_SIZE, DIRECT_MAP_OFFSET};
use core::ptr::{read_volatile, write_volatile, addr_of, addr_of_mut};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::drivers::input::hid::hid::{BootKeyboardReport, BootMouseReport};
use crate::drivers::input::hid::keyboard;
use crate::drivers::input::hid::mouse;

// ============================================================================
// 寄存器定义
// ============================================================================

/// 能力寄存器 (Capability Registers)
#[repr(C, packed)]
struct CapabilityRegisters {
    /// Capability Register Length
    caplength: u8,
    /// Reserved
    reserved: u8,
    /// Interface Version Number
    hciversion: u16,
    /// Structural Parameters 1
    hcsparams1: u32,
    /// Structural Parameters 2
    hcsparams2: u32,
    /// Structural Parameters 3
    hcsparams3: u32,
    /// Capability Parameters 1
    hccparams1: u32,
    /// Doorbell Offset
    dboff: u32,
    /// Runtime Register Space Offset
    rtsoff: u32,
    /// Capability Parameters 2
    hccparams2: u32,
}

/// 操作寄存器 (Operational Registers)
#[repr(C)]
struct OperationalRegisters {
    /// USB Command
    usbcmd: u32,
    /// USB Status
    usbsts: u32,
    /// Page Size
    pagesize: u32,
    _pad1: [u32; 2],
    /// Device Notification Control
    dnctrl: u32,
    /// Command Ring Control
    crcr: u64,
    _pad2: [u32; 4],
    /// Device Context Base Address Array Pointer
    dcbaap: u64,
    /// Configure
    config: u32,
}

/// 运行时寄存器组 (Runtime Registers)
/// 实际上是 Interrupter Register Set 的数组
#[repr(C)]
struct RuntimeRegisters {
    mfindex: u32,
    reserved: [u32; 7],
    irs: [InterrupterRegisters; 0], // 动态长度，通常至少有 1 个
}

/// 中断寄存器集 (Interrupter Register Set)
#[repr(C)]
struct InterrupterRegisters {
    iman: u32, // Management
    imod: u32, // Moderation
    erstsz: u32, // Segment Table Size
    reserved: u32,
    erstba: u64, // Segment Table Base Address
    erdp: u64, // Dequeue Pointer
}

/// 门铃寄存器组 (Doorbell Registers)
#[repr(C)]
struct DoorbellRegisters {
    doorbells: [u32; 0], // 动态长度，MaxSlots + 1
}

// 数据结构定义

/// 传输请求块 (Transfer Request Block)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Trb {
    parameter: u64,
    status: u32,
    control: u32,
}

// ============================================================================
// Context Structures
// ============================================================================

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct SlotContext {
    /// Route String (20-0), Speed (23-20), MTT (25), Hub (26), Context Entries (31-27)
    info1: u32,
    /// Max Exit Latency (15-0), Root Hub Port Number (23-16), Number of Ports (31-24)
    info2: u32,
    /// Parent Hub Slot ID (7-0), Parent Port Number (15-8), TT Think Time (17-16), Interrupter Target (31-22)
    tt_id: u32,
    /// Device Address (7-0), Slot State (31-27)
    state: u32,
    reserved: [u32; 4],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct EndpointContext {
    /// EP State (2-0), Mult (9-8), MaxPStreams (14-10), LSA (15), Interval (23-16)
    info1: u32,
    /// Force Event (0), CErr (2-1), EP Type (5-3), HID (7), Max Burst Size (15-8), Max Packet Size (31-16)
    info2: u32,
    /// Dequeue Cycle State (0), TR Dequeue Pointer (31-4)
    tr_dequeue_ptr_lo: u32,
    tr_dequeue_ptr_hi: u32,
    /// Average TRB Length (15-0), Max ESIT Payload (31-16)
    average_trb_len: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct DeviceContext {
    slot: SlotContext,
    endpoints: [EndpointContext; 31],
}

#[repr(C)]
struct InputControlContext {
    drop_flags: u32,
    add_flags: u32,
    reserved: [u32; 5],
    config: u32,
}

#[repr(C)]
struct InputContext {
    control: InputControlContext,
    slot: SlotContext,
    endpoints: [EndpointContext; 31],
}

/// 事件环段表条目 (Event Ring Segment Table Entry)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct EventRingSegmentTableEntry {
    base_addr: u64,
    size: u16,
    reserved: u16,
    reserved2: u32,
}

// USBCMD 位定义
const USBCMD_RUN_STOP: u32 = 1 << 0;
const USBCMD_RESET: u32 = 1 << 1;
const USBCMD_INTE: u32 = 1 << 2;
const USBCMD_HSEE: u32 = 1 << 3;
const USBCMD_LHCRST: u32 = 1 << 7; // Light HC Reset

// USBSTS 位定义
const USBSTS_HCH: u32 = 1 << 0; // HC Halted
const USBSTS_HSE: u32 = 1 << 2; // Host System Error
const USBSTS_EINT: u32 = 1 << 3; // Event Interrupt
const USBSTS_PCD: u32 = 1 << 4; // Port Change Detect
const USBSTS_CNR: u32 = 1 << 11; // Controller Not Ready

// CRCR 位定义
const CRCR_RCS: u64 = 1 << 0; // Ring Cycle State
const CRCR_CS: u64 = 1 << 1;  // Command Stop
const CRCR_CA: u64 = 1 << 2;  // Command Abort
const CRCR_CRR: u64 = 1 << 3; // Command Ring Running

// PORTSC 位定义
const PORTSC_CCS: u32 = 1 << 0; // Current Connect Status
const PORTSC_PED: u32 = 1 << 1; // Port Enabled/Disabled
const PORTSC_PR: u32 = 1 << 4;  // Port Reset
const PORTSC_PP: u32 = 1 << 9;  // Port Power
const PORTSC_CSC: u32 = 1 << 17; // Connect Status Change
const PORTSC_PRC: u32 = 1 << 21; // Port Reset Change

// TRB Types
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
const TRB_TYPE_NO_OP: u32 = 23;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_CMD_COMPLETION: u32 = 33;
const TRB_TYPE_PORT_STATUS_CHANGE: u32 = 34;

// TRB Completion Codes
const TRB_CC_SUCCESS: u32 = 1;
const TRB_CC_SHORT_PACKET: u32 = 13;

// ============================================================================
// USB Standard Structures
// ============================================================================

#[repr(C, packed)]
struct SetupPacket {
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
struct DeviceDescriptor {
    length: u8,
    descriptor_type: u8,
    bcd_usb: u16,
    device_class: u8,
    device_subclass: u8,
    device_protocol: u8,
    max_packet_size0: u8,
    id_vendor: u16,
    id_product: u16,
    bcd_device: u16,
    manufacturer: u8,
    product: u8,
    serial_number: u8,
    num_configurations: u8,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
struct ConfigurationDescriptor {
    length: u8,
    descriptor_type: u8,
    total_length: u16,
    num_interfaces: u8,
    configuration_value: u8,
    configuration_index: u8,
    attributes: u8,
    max_power: u8,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
struct InterfaceDescriptor {
    length: u8,
    descriptor_type: u8,
    interface_number: u8,
    alternate_setting: u8,
    num_endpoints: u8,
    interface_class: u8,
    interface_subclass: u8,
    interface_protocol: u8,
    interface_string: u8,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
struct EndpointDescriptor {
    length: u8,
    descriptor_type: u8,
    endpoint_address: u8,
    attributes: u8,
    max_packet_size: u16,
    interval: u8,
}

#[derive(Clone, Copy, Debug)]
struct RingState {
    phys: u64,
    virt: *mut Trb,
    enqueue_idx: usize,
    cycle_state: u32,
}

#[derive(Clone, Copy, Debug)]
struct EndpointState {
    ring: RingState,
    // Buffer for Interrupt transfers (reused)
    buffer: Option<(u64, *mut u8, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeviceType {
    Unknown,
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, Debug)]
struct SlotState {
    // Index is DCI (Device Context Index). 1 = EP0.
    endpoints: [Option<EndpointState>; 32],
    device_type: DeviceType,
}

// ============================================================================
// 控制器结构
// ============================================================================

struct XhciController {
    pci_addr: PciAddress,
    mmio_base: *mut u8,
    mmio_size: usize,
    cap_regs: *const CapabilityRegisters,
    op_regs: *mut OperationalRegisters,
    rt_regs: *mut RuntimeRegisters,
    db_regs: *mut DoorbellRegisters,
    
    // Capabilities
    max_slots: u8,
    max_ports: u8,
    context_size_64: bool,
    
    // Data Structures
    dcbaap_phys: u64,
    dcbaap_virt: *mut u64,
    
    cmd_ring_phys: u64,
    cmd_ring_virt: *mut Trb,
    cmd_ring_enqueue_idx: usize,
    cmd_ring_cycle_state: u32,
    
    event_ring_phys: u64,
    event_ring_virt: *mut Trb,
    event_ring_dequeue_idx: usize,
    event_ring_cycle_state: u32,
    
    erst_phys: u64,

    // Slot Management
    slots: [Option<SlotState>; 32],
}

static mut XHCI_CONTROLLER: Option<XhciController> = None;

impl XhciController {
    unsafe fn enqueue_command(&mut self, param: u64, status: u32, control: u32) {
        let mut idx = self.cmd_ring_enqueue_idx;
        
        // Check for Link TRB (assume ring size 256, last one is Link)
        if idx == 255 {
            let link_trb = self.cmd_ring_virt.add(255);
            let mut link_control = (TRB_TYPE_LINK << 10) | (1 << 1); // Type Link, TC
            if self.cmd_ring_cycle_state != 0 {
                link_control |= 1; // Set Cycle bit
            }
            
            (*link_trb).parameter = self.cmd_ring_phys;
            (*link_trb).status = 0;
            write_volatile(&mut (*link_trb).control, link_control);
            
            // Toggle Cycle State
            self.cmd_ring_cycle_state ^= 1;
            idx = 0;
        }
        
        let trb = self.cmd_ring_virt.add(idx);
        (*trb).parameter = param;
        (*trb).status = status;
        
        let mut final_control = control;
        if self.cmd_ring_cycle_state != 0 {
            final_control |= 1; // Set Cycle bit
        } else {
            final_control &= !1; // Clear Cycle bit
        }
        
        write_volatile(&mut (*trb).control, final_control);
        
        self.cmd_ring_enqueue_idx = idx + 1;
    }
    
    unsafe fn ring_doorbell(&mut self, slot: u8) {
        let db_reg = (self.db_regs as *mut u32).add(slot as usize);
        write_volatile(db_reg, 0);
    }

    unsafe fn ring_doorbell_ep(&mut self, slot: u8, dci: u8) {
        let db_reg = (self.db_regs as *mut u32).add(slot as usize);
        write_volatile(db_reg, dci as u32);
    }

    unsafe fn enqueue_transfer(&mut self, slot_id: u8, dci: u8, param: u64, status: u32, control: u32) {
        if let Some(slot_state) = &mut self.slots[slot_id as usize] {
            if let Some(ep_state) = &mut slot_state.endpoints[dci as usize] {
                let ring = &mut ep_state.ring;
                let mut idx = ring.enqueue_idx;
                
                // Check for Link TRB (assume ring size 256, last one is Link)
                if idx == 255 {
                    let link_trb = ring.virt.add(255);
                    let mut link_control = (TRB_TYPE_LINK << 10) | (1 << 1); // Type Link, TC
                    if ring.cycle_state != 0 {
                        link_control |= 1; // Set Cycle bit
                    }
                    
                    (*link_trb).parameter = ring.phys;
                    (*link_trb).status = 0;
                    write_volatile(&mut (*link_trb).control, link_control);
                    
                    // Toggle Cycle State
                    ring.cycle_state ^= 1;
                    idx = 0;
                }
                
                let trb = ring.virt.add(idx);
                (*trb).parameter = param;
                (*trb).status = status;
                
                let mut final_control = control;
                if ring.cycle_state != 0 {
                    final_control |= 1; // Set Cycle bit
                } else {
                    final_control &= !1; // Clear Cycle bit
                }
                
                write_volatile(&mut (*trb).control, final_control);
                
                ring.enqueue_idx = idx + 1;
            }
        }
    }
    
    unsafe fn wait_for_event(&mut self, trb_type: u32, timeout_ms: usize) -> Option<Trb> {
        let mut timeout = timeout_ms * 1000;
        while timeout > 0 {
            if let Some(trb) = self.poll_event() {
                let type_ = (trb.control >> 10) & 0x3F;
                if type_ == trb_type {
                    return Some(trb);
                }
                // kprintln!("USB: Ignored event type {}", type_);
            }
            
            for _ in 0..100 { core::hint::spin_loop(); }
            timeout -= 1;
        }
        None
    }
    
    unsafe fn poll_event(&mut self) -> Option<Trb> {
        let idx = self.event_ring_dequeue_idx;
        let trb_ptr = self.event_ring_virt.add(idx);
        let trb = read_volatile(trb_ptr);
        
        let cycle_bit = (trb.control & 1) as u32;
        
        if cycle_bit == self.event_ring_cycle_state {
            // Event available
            self.event_ring_dequeue_idx += 1;
            if self.event_ring_dequeue_idx == 256 {
                self.event_ring_dequeue_idx = 0;
                self.event_ring_cycle_state ^= 1;
            }
            
            // Update ERDP
            let ir_set = addr_of_mut!((*self.rt_regs).irs) as *mut InterrupterRegisters;
            let ir0 = &mut *ir_set;
            let er_seg_phys = self.event_ring_phys;
            let new_dequeue_ptr = er_seg_phys + (self.event_ring_dequeue_idx as u64 * 16);
            
            write_volatile(&mut ir0.erdp, new_dequeue_ptr | (1 << 3));
            
            return Some(trb);
        }
        
        None
    }

    unsafe fn handle_transfer_event(&mut self, trb: &Trb) {
        let slot_id = (trb.control >> 24) & 0xFF;
        let dci = (trb.control >> 16) & 0x1F;
        let cc = (trb.status >> 24) & 0xFF;
        
        if cc != TRB_CC_SUCCESS && cc != TRB_CC_SHORT_PACKET {
             warn!("USB: Transfer Event Failed Slot {} DCI {} CC {}", slot_id, dci, cc);
             return;
        }
        
        if let Some(slot_state) = &mut self.slots[slot_id as usize] {
            if let Some(ep_state) = &mut slot_state.endpoints[dci as usize] {
                if let Some((_, buf_virt, len)) = ep_state.buffer {
                     // Data received!
                     // kprintln!("USB: Data received from Slot {} DCI {}", slot_id, dci);
                     
                     // Print first few bytes
                     let slice = core::slice::from_raw_parts(buf_virt, len);
                     // kprintln!("USB: Data: {:02x?}", slice);
                     
                     // If it is HID data, we could parse it
                     // Assume DCI 3 = EP1 IN (Interrupt)
                     if dci == 3 {
                         match slot_state.device_type {
                             DeviceType::Keyboard => {
                                 if slice.len() >= 8 {
                                     let report = &*(buf_virt as *const BootKeyboardReport);
                                     // kprint!("Keyboard [Slot {}]: Mods={:02x} Keys=[", slot_id, report.modifiers);
                                     // for k in report.keycodes.iter() {
                                     //     if *k != 0 { kprint!("{:02x} ", k); }
                                     // }
                                     // kprintln!("]");
                                     
                                     // Send to HID subsystem
                                     keyboard::handle_boot_report(*report);
                                 }
                             },
                             DeviceType::Mouse => {
                                 if slice.len() >= 3 {
                                     let buttons = slice[0];
                                     let x = slice[1] as i8;
                                     let y = slice[2] as i8;
                                     let wheel = if slice.len() >= 4 { slice[3] as i8 } else { 0 };
                                     // kprintln!("Mouse [Slot {}]: Btn={:02x} X={} Y={} Wheel={}", 
                                     //     slot_id, buttons, x, y, wheel);
                                     
                                     let report = BootMouseReport {
                                         buttons,
                                         x,
                                         y,
                                         wheel,
                                     };
                                     
                                     // Send to HID subsystem
                                     mouse::handle_boot_report(report);
                                 }
                             },
                             _ => {
                                 // Just print for now
                                 debug!("USB Input [Slot {}]: {:02x?}", slot_id, slice);
                             }
                         }
                     }
                     
                     // Re-queue transfer
                     let trb_type = 1; // Normal TRB
                     let control = (trb_type << 10) | (1 << 5) | (1 << 2); // IOC, ISP
                     
                     // Reuse buffer
                     let (phys, _, len) = ep_state.buffer.unwrap();
                     
                     // Need to call enqueue_transfer but we have mutable borrow of slot_state
                     // We cannot call self.enqueue_transfer directly because of borrow checker?
                     // self is borrowed mutably.
                     // But we have reference to slot_state inside self.
                     // We need to access ring inside ep_state.
                     
                     let ring = &mut ep_state.ring;
                     let mut idx = ring.enqueue_idx;
                     
                     if idx == 255 {
                        let link_trb = ring.virt.add(255);
                        let mut link_control = (TRB_TYPE_LINK << 10) | (1 << 1);
                        if ring.cycle_state != 0 { link_control |= 1; }
                        (*link_trb).parameter = ring.phys;
                        (*link_trb).status = 0;
                        write_volatile(&mut (*link_trb).control, link_control);
                        ring.cycle_state ^= 1;
                        idx = 0;
                     }
                     
                     let trb = ring.virt.add(idx);
                     (*trb).parameter = phys;
                     (*trb).status = len as u32; // Transfer Length
                     
                     let mut final_control = control;
                     if ring.cycle_state != 0 { final_control |= 1; } else { final_control &= !1; }
                     write_volatile(&mut (*trb).control, final_control);
                     
                     ring.enqueue_idx = idx + 1;
                     
                     // Doorbell
                     // We can't call self.ring_doorbell_ep either.
                     // Calculate address manually.
                     let db_reg = (self.db_regs as *mut u32).add(slot_id as usize);
                     write_volatile(db_reg, dci as u32);
                }
            }
        }
    }

    unsafe fn enable_slot(&mut self) -> Option<u8> {

        kprintln!("USB: Sending Enable Slot Command...");
        
        // Enable Slot Command: Type = 9
        let cmd_trb_control = (TRB_TYPE_ENABLE_SLOT << 10);
        self.enqueue_command(0, 0, cmd_trb_control);
        
        self.ring_doorbell(0);
        
        // Wait for Command Completion Event
        if let Some(event) = self.wait_for_event(TRB_TYPE_CMD_COMPLETION, 1000) {
            let completion_code = (event.status >> 24) & 0xFF;
            if completion_code == TRB_CC_SUCCESS {
                let slot_id = (event.control >> 24) & 0xFF;
                kprintln!("USB: Slot Enabled, ID: {}", slot_id);
                return Some(slot_id as u8);
            } else {
                kprintln!("USB: Enable Slot Failed, CC: {}", completion_code);
            }
        } else {
            kprintln!("USB: Enable Slot Timeout");
        }
        
        None
    }

    unsafe fn address_device(&mut self, slot_id: u8, port_id: u8, speed: u32) -> bool {
        kprintln!("USB: Addressing Device on Slot {} (Port {}, Speed {})", slot_id, port_id, speed);
        
        // 1. Allocate Device Context
        let (dc_phys, dc_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return false,
        };
        
        // Update DCBAAP
        let dcbaap = self.dcbaap_virt;
        *dcbaap.add(slot_id as usize) = dc_phys;
        
        // 2. Allocate Input Context
        let (ic_phys, ic_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return false,
        };
        
        let input_ctx = &mut *(ic_virt as *mut InputContext);
        
        // 3. Setup Input Control Context
        // Add Flags: Slot Context (Bit 0) | Endpoint 0 Context (Bit 1)
        input_ctx.control.add_flags = (1 << 0) | (1 << 1);
        
        // 4. Setup Slot Context
        // Info1: Speed, Context Entries (1 for EP0)
        input_ctx.slot.info1 |= (speed << 20);
        input_ctx.slot.info1 |= (1 << 27); // Context Entries = 1
        
        // Info2: Root Hub Port Number
        input_ctx.slot.info2 |= (port_id as u32) << 16;
        
        // 5. Setup Endpoint 0 Context
        // EP Type = Control (Value 4)
        // Max Packet Size: Super=512, High=64, Others=8
        let mps = match speed {
            4 => 512,
            3 => 64,
            _ => 8,
        };
        
        input_ctx.endpoints[0].info2 |= (4 << 3); // EP Type = Control
        input_ctx.endpoints[0].info2 |= (mps << 16); // Max Packet Size
        input_ctx.endpoints[0].info2 |= (3 << 1); // CErr = 3
        
        // Allocate Transfer Ring for EP0
        let (tr_phys, tr_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return false,
        };
        
        // Initialize Link TRB at end of Transfer Ring
        let link_trb = (tr_virt as *mut Trb).add(255);
        let link_control = (TRB_TYPE_LINK << 10) | (1 << 1); // Link, TC
        (*link_trb).parameter = tr_phys;
        write_volatile(&mut (*link_trb).control, link_control);
        
        // Save Slot State
        let mut endpoints = [None; 32];
        endpoints[1] = Some(EndpointState {
            ring: RingState {
                phys: tr_phys,
                virt: tr_virt as *mut Trb,
                enqueue_idx: 0,
                cycle_state: 1,
            },
            buffer: None,
        });
        
        self.slots[slot_id as usize] = Some(SlotState {
            endpoints,
            device_type: DeviceType::Unknown,
        });
        
        // Set Dequeue Pointer in EP Context (DCS=1)
        input_ctx.endpoints[0].tr_dequeue_ptr_lo = (tr_phys as u32) | 1;
        input_ctx.endpoints[0].tr_dequeue_ptr_hi = (tr_phys >> 32) as u32;
        
        // 6. Issue Address Device Command
        let cmd_control = (TRB_TYPE_ADDRESS_DEVICE << 10) | ((slot_id as u32) << 24);
        self.enqueue_command(ic_phys, 0, cmd_control);
        self.ring_doorbell(0);
        
        // Wait for completion
        if let Some(event) = self.wait_for_event(TRB_TYPE_CMD_COMPLETION, 1000) {
            let cc = (event.status >> 24) & 0xFF;
            if cc == TRB_CC_SUCCESS {
                kprintln!("USB: Device Addressed on Slot {}", slot_id);
                return true;
            } else {
                kprintln!("USB: Address Device Failed, CC: {}", cc);
            }
        } else {
            kprintln!("USB: Address Device Timeout");
        }
        
        false
    }

    unsafe fn send_control_transfer(&mut self, slot_id: u8, setup: SetupPacket, buffer: Option<(u64, usize)>) -> bool {
        // 1. Setup Stage
        let setup_ptr = addr_of!(setup) as *const u32;
        let setup_trb_param_low = read_volatile(setup_ptr);
        let setup_trb_param_high = read_volatile(setup_ptr.add(1));
        
        // TRB Type = Setup Stage (2)
        // IDT = 1 (Immediate Data)
        // TRT = 2 (IN Data Stage) or 3 (OUT Data Stage) or 0 (No Data Stage)
        let trt = if setup.length > 0 {
            if (setup.request_type & 0x80) != 0 { 3 } else { 2 } // 3=IN, 2=OUT
        } else {
            0
        };
        
        let setup_control = (TRB_TYPE_SETUP_STAGE << 10) | (1 << 6) | (trt << 16);
        self.enqueue_transfer(slot_id, 1, (setup_trb_param_high as u64) << 32 | (setup_trb_param_low as u64), 8, setup_control);
        
        // 2. Data Stage (Optional)
        if let Some((buf_phys, buf_len)) = buffer {
            let dir_in = (setup.request_type & 0x80) != 0;
            let direction_bit = if dir_in { 1 << 16 } else { 0 };
            
            let data_control = (TRB_TYPE_DATA_STAGE << 10) | direction_bit;
            self.enqueue_transfer(slot_id, 1, buf_phys, buf_len as u32, data_control);
        }
        
        // 3. Status Stage
        // Direction is opposite to Data Stage
        // If No Data Stage, Direction is IN (1)
        let dir_in = if setup.length > 0 {
            (setup.request_type & 0x80) == 0 // Opposite: Data IN -> Status OUT, Data OUT -> Status IN
        } else {
            true // No Data -> Status IN
        };
        
        let direction_bit = if dir_in { 1 << 16 } else { 0 };
        let status_control = (TRB_TYPE_STATUS_STAGE << 10) | direction_bit | (1 << 5); // IOC (Interrupt On Completion)
        
        self.enqueue_transfer(slot_id, 1, 0, 0, status_control);
        
        // Ring Doorbell for Slot, EP0 (DCI = 1)
        self.ring_doorbell_ep(slot_id, 1);
        
        // Wait for Completion
        if let Some(event) = self.wait_for_event(TRB_TYPE_TRANSFER_EVENT, 1000) {
             let cc = (event.status >> 24) & 0xFF;
             if cc == TRB_CC_SUCCESS {
                 return true;
             }
             kprintln!("USB: Control Transfer Failed, CC: {}", cc);
        } else {
            kprintln!("USB: Control Transfer Timeout");
        }
        
        false
    }

    unsafe fn set_protocol(&mut self, slot_id: u8, interface_num: u8, protocol: u16) -> bool {
        let setup = SetupPacket {
            request_type: 0x21,
            request: 0x0B,
            value: protocol,
            index: interface_num as u16,
            length: 0,
        };
        self.send_control_transfer(slot_id, setup, None)
    }

    unsafe fn set_idle(&mut self, slot_id: u8, interface_num: u8, duration: u8, report_id: u8) -> bool {
        let setup = SetupPacket {
            request_type: 0x21,
            request: 0x0A,
            value: ((duration as u16) << 8) | (report_id as u16),
            index: interface_num as u16,
            length: 0,
        };
        self.send_control_transfer(slot_id, setup, None)
    }

    unsafe fn configure_device(&mut self, slot_id: u8) {
        kprintln!("USB: Configuring Device on Slot {}", slot_id);
        
        // Allocate buffer for Descriptor
        let (buf_phys, buf_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return,
        };
        
        // 1. Get Device Descriptor (First 8 bytes)
        let setup = SetupPacket {
            request_type: 0x80, // Device to Host, Standard, Device
            request: 6, // GET_DESCRIPTOR
            value: 1 << 8, // Descriptor Type (1=Device) << 8 | Index (0)
            index: 0,
            length: 8,
        };
        
        if !self.send_control_transfer(slot_id, setup, Some((buf_phys, 8))) {
            return;
        }

        let desc = &*(buf_virt as *const DeviceDescriptor);
        let mps = desc.max_packet_size0;
        kprintln!("USB: Slot {} MaxPacketSize0: {}", slot_id, mps);
             
        // 2. Get Full Device Descriptor
        let setup_full = SetupPacket {
            request_type: 0x80,
            request: 6,
            value: 1 << 8,
            index: 0,
            length: 18,
        };
             
        if !self.send_control_transfer(slot_id, setup_full, Some((buf_phys, 18))) {
            return;
        }

        let desc = &*(buf_virt as *const DeviceDescriptor);
        let vendor = desc.id_vendor;
        let product = desc.id_product;
                 
        kprintln!("USB: Slot {} Vendor: {:04x}, Product: {:04x}", 
            slot_id, vendor, product);
            
        if vendor == 0x0627 && product == 0x0001 {
             kprintln!("USB: Found QEMU USB Tablet/Mouse");
        } else if vendor == 0x046d {
             kprintln!("USB: Found Logitech Device");
        }

        // 3. Get Configuration Descriptor Header (9 bytes)
        let setup_conf = SetupPacket {
            request_type: 0x80,
            request: 6, // GET_DESCRIPTOR
            value: 2 << 8, // Type 2 (Configuration) | Index 0
            index: 0,
            length: 9,
        };
        
        if self.send_control_transfer(slot_id, setup_conf, Some((buf_phys, 9))) {
             let conf = &*(buf_virt as *const ConfigurationDescriptor);
             let total_len = conf.total_length;
             kprintln!("USB: Slot {} Config Total Length: {}", slot_id, total_len);
             
             // 4. Get Full Configuration Descriptor
             let setup_full_conf = SetupPacket {
                request_type: 0x80,
                request: 6,
                value: 2 << 8,
                index: 0,
                length: total_len,
             };
             
             if self.send_control_transfer(slot_id, setup_full_conf, Some((buf_phys, total_len as usize))) {
                 // Parse descriptors
                 self.parse_configuration(slot_id, buf_virt, total_len as usize);
             }
        }
    }

    unsafe fn parse_configuration(&mut self, slot_id: u8, buffer: *mut u8, length: usize) {
        let mut offset = 0;
        let mut current_interface = 0;
        
        // Find Endpoints to configure
        let mut endpoints_to_config = [None; 16]; // Store EndpointDescriptors
        let mut ep_count = 0;

        while offset < length {
            let header = buffer.add(offset);
            let len = *header;
            let type_ = *header.add(1);
            
            if len == 0 { break; } 
            
            if type_ == 4 { // Interface Descriptor
                let if_desc = &*(header as *const InterfaceDescriptor);
                current_interface = if_desc.interface_number;
                let class = if_desc.interface_class;
                let subclass = if_desc.interface_subclass;
                let protocol = if_desc.interface_protocol;
                
                kprintln!("USB: Interface {} Class: {} Subclass: {} Protocol: {}", 
                    current_interface, class, subclass, protocol);
                
                if class == 3 { // HID
                     kprintln!("USB: Found HID Interface");
                     
                     {
                         if let Some(slot_state) = &mut self.slots[slot_id as usize] {
                             if protocol == 1 {
                                 slot_state.device_type = DeviceType::Keyboard;
                                 kprintln!("USB: Device Type: Keyboard");
                             } else if protocol == 2 {
                                 slot_state.device_type = DeviceType::Mouse;
                                 kprintln!("USB: Device Type: Mouse");
                             }
                         }
                     }
                     
                     if subclass == 1 {
                         kprintln!("USB: Setting Boot Protocol and Idle for Interface {}", current_interface);
                         self.set_protocol(slot_id, current_interface, 0);
                         self.set_idle(slot_id, current_interface, 0, 0);
                     }
                }
            } else if type_ == 5 { // Endpoint Descriptor
                let ep_desc = &*(header as *const EndpointDescriptor);
                let addr = ep_desc.endpoint_address;
                let attr = ep_desc.attributes;
                
                kprintln!("USB: Endpoint Addr: {:#x} Attr: {:#x}", addr, attr);
                
                // If Interrupt IN
                if (addr & 0x80) != 0 && (attr & 0x03) == 3 {
                    if ep_count < 16 {
                        endpoints_to_config[ep_count] = Some(*ep_desc);
                        ep_count += 1;
                    }
                }
            }
            
            offset += len as usize;
        }

        // Configure Endpoints via xHCI
        for i in 0..ep_count {
            if let Some(ep_desc) = endpoints_to_config[i] {
                self.configure_endpoint_xhci(slot_id, &ep_desc);
            }
        }
        
        // SET_CONFIGURATION (USB Request)
        let setup_set_conf = SetupPacket {
            request_type: 0x00, 
            request: 9, // SET_CONFIGURATION
            value: 1, 
            index: 0,
            length: 0,
        };
        if self.send_control_transfer(slot_id, setup_set_conf, None) {
            kprintln!("USB: Device Configured");
            
            // Start Polling for Interrupt Endpoints
             for i in 0..ep_count {
                if let Some(ep_desc) = endpoints_to_config[i] {
                    let ep_num = ep_desc.endpoint_address & 0x0F;
                    let dir_in = (ep_desc.endpoint_address & 0x80) != 0;
                    let dci = (ep_num * 2) + if dir_in { 1 } else { 0 };
                    
                    self.queue_interrupt_transfer(slot_id, dci);
                }
            }
        }
    }

    unsafe fn configure_endpoint_xhci(&mut self, slot_id: u8, ep_desc: &EndpointDescriptor) -> bool {
         let ep_num = ep_desc.endpoint_address & 0x0F;
         let dir_in = (ep_desc.endpoint_address & 0x80) != 0;
         let dci = (ep_num * 2) + if dir_in { 1 } else { 0 };
         
         kprintln!("USB: Configuring Endpoint {}, DCI: {}", ep_num, dci);
         
         // 1. Allocate Transfer Ring
         let (tr_phys, tr_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return false,
        };
        
        // Link TRB
        let link_trb = (tr_virt as *mut Trb).add(255);
        let link_control = (TRB_TYPE_LINK << 10) | (1 << 1);
        (*link_trb).parameter = tr_phys;
        write_volatile(&mut (*link_trb).control, link_control);
        
        let ring = RingState {
            phys: tr_phys,
            virt: tr_virt as *mut Trb,
            enqueue_idx: 0,
            cycle_state: 1,
        };

        // Save Ring State
        if let Some(slot_state) = &mut self.slots[slot_id as usize] {
            slot_state.endpoints[dci as usize] = Some(EndpointState {
                ring,
                buffer: None,
            });
        }
        
        // 2. Setup Input Context
        let (ic_phys, ic_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return false,
        };
        let input_ctx = &mut *(ic_virt as *mut InputContext);
        
        // Add Flags: DCI | Slot Context
        input_ctx.control.add_flags = (1 << dci) | (1 << 0); 
        
        // Slot Context: Context Entries
        input_ctx.slot.info1 |= (31 << 27); 
        
        // Endpoint Context
        let ep_ctx = &mut input_ctx.endpoints[dci as usize - 1];
        
        let ep_type = match (ep_desc.attributes & 0x03, dir_in) {
            (0, _) => 4, // Control
            (1, false) => 1, (1, true) => 5,
            (2, false) => 2, (2, true) => 6,
            (3, false) => 3, (3, true) => 7,
            _ => 0,
        };
        
        ep_ctx.info2 |= ((ep_type as u32) << 3);
        ep_ctx.info2 |= ((ep_desc.max_packet_size as u32) << 16);
        
        // Interval: simplified
        ep_ctx.info1 |= (6 << 16); 
        
        ep_ctx.info2 |= (3 << 1); // CErr
        
        // Dequeue Pointer
        ep_ctx.tr_dequeue_ptr_lo = (tr_phys as u32) | 1; // DCS=1
        ep_ctx.tr_dequeue_ptr_hi = (tr_phys >> 32) as u32;
        
        ep_ctx.average_trb_len = 8;
        
        // 3. Issue Configure Endpoint Command
        let cmd_control = (TRB_TYPE_CONFIGURE_ENDPOINT << 10) | ((slot_id as u32) << 24);
        self.enqueue_command(ic_phys, 0, cmd_control);
        self.ring_doorbell(0);
        
        if let Some(event) = self.wait_for_event(TRB_TYPE_CMD_COMPLETION, 1000) {
             let cc = (event.status >> 24) & 0xFF;
             if cc == TRB_CC_SUCCESS {
                 return true;
             } else {
                 kprintln!("USB: Configure Endpoint Failed, CC: {}", cc);
             }
        }
        
        false
    }

    unsafe fn queue_interrupt_transfer(&mut self, slot_id: u8, dci: u8) {
        // Allocate buffer for data
        let (buf_phys, buf_virt) = match alloc_dma_zeroed() {
            Some(x) => x,
            None => return,
        };
        
        // Save Buffer in EndpointState
        if let Some(slot_state) = &mut self.slots[slot_id as usize] {
            if let Some(ep_state) = &mut slot_state.endpoints[dci as usize] {
                ep_state.buffer = Some((buf_phys, buf_virt, 4096));
            }
        }
        
        // Normal TRB
        // Type = Normal (1)
        // IOC = 1
        // Length = 8 (for mouse/keyboard usually enough)
        
        let trb_type = 1; // Normal TRB
        let length = 8;
        let control = (trb_type << 10) | (1 << 5) | (1 << 2); // IOC, ISP (Interrupt on Short Packet)
        
        self.enqueue_transfer(slot_id, dci, buf_phys, length, control);
        self.ring_doorbell_ep(slot_id, dci);
        
        kprintln!("USB: Queued Interrupt Transfer for Slot {} DCI {}", slot_id, dci);
    }
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// 列出所有连接的 USB 设备
pub fn dump_devices() {
    unsafe {
        if let Some(xhci) = &*core::ptr::addr_of!(XHCI_CONTROLLER) {
            crate::kprintln!("USB Devices (Max Slots: {}):", xhci.max_slots);
            let mut found = false;
            // 遍历所有可能的 Slot
            for i in 1..=xhci.max_slots {
                 if i as usize >= xhci.slots.len() {
                     break;
                 }
                 if let Some(slot) = &xhci.slots[i as usize] {
                     let type_str = match slot.device_type {
                         DeviceType::Keyboard => "HID Keyboard",
                         DeviceType::Mouse => "HID Mouse",
                         DeviceType::Unknown => "Unknown Device",
                     };
                     crate::kprintln!("  Slot {}: {}", i, type_str);
                     found = true;
                 }
            }
            if !found {
                crate::kprintln!("  No devices connected.");
            }
        } else {
            crate::kprintln!("USB xHCI Controller not initialized.");
        }
    }
}

/// 轮询 USB 事件
pub fn poll() {
    unsafe {
        if let Some(ref mut controller) = XHCI_CONTROLLER {
            // Process up to 256 events per poll to avoid starvation and ring overflow
            for _ in 0..256 {
                if let Some(trb) = controller.poll_event() {
                    let type_ = (trb.control >> 10) & 0x3F;
                    if type_ == TRB_TYPE_TRANSFER_EVENT {
                        controller.handle_transfer_event(&trb);
                    }
                } else {
                    break;
                }
            }
        }
    }
}

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 xHCI 控制器
pub fn init() {
    // 扫描 PCI 总线查找 xHCI 控制器
    pci::scan_bus(&mut |addr, header| {
        // Class 0x0C (Serial Bus Controller)
        // Subclass 0x03 (USB Controller)
        // ProgIF 0x30 (xHCI)
        if header.class_code == 0x0C && header.subclass == 0x03 && header.prog_if == 0x30 {
            kprintln!("USB: Found xHCI Controller at {:?} (Vendor: {:04x}, Device: {:04x})", 
                addr, header.vendor_id, header.device_id);
            
            // 初始化控制器
            if !INITIALIZED.load(Ordering::Relaxed) {
                INITIALIZED.store(true, Ordering::Relaxed);
                unsafe {
                    init_controller(addr, header);
                }
            }
        }
    });
}

unsafe fn init_controller(addr: PciAddress, header: PciHeader) {
    // 1. 启用 Bus Master 和 Memory Space
    let cmd = header.command | 0x06; // Bit 1 (Memory), Bit 2 (Bus Master)
    pci::write_config_32(addr, 0x04, cmd as u32);
    
    // 2. 读取 BAR0 获取 MMIO 基地址
    let bar0 = pci::read_config_32(addr, 0x10);
    let bar_type = (bar0 >> 1) & 0x03;
    let prefetchable = (bar0 >> 3) & 0x01;
    
    let mut mmio_phys = (bar0 & 0xFFFF_FFF0) as u64;
    
    // 如果是 64 位 BAR，读取 BAR1
    if bar_type == 0x02 {
        let bar1 = pci::read_config_32(addr, 0x14);
        mmio_phys |= (bar1 as u64) << 32;
    }
    
    kprintln!("USB: xHCI MMIO Phys Base: {:#x}", mmio_phys);
    
    // 3. 映射 MMIO
    // 暂时映射 64KB，通常足够覆盖寄存器
    let mmio_size = 64 * 1024;
    let mmio_base = ioremap(mmio_phys, mmio_size);
    
    if mmio_base.is_null() {
        kprintln!("USB: Failed to map xHCI MMIO");
        return;
    }
    
    kprintln!("USB: xHCI MMIO Mapped at {:#x}", mmio_base as u64);
    
    // 4. 解析寄存器
    let cap_regs = mmio_base as *const CapabilityRegisters;
    let caplength = read_volatile(addr_of!((*cap_regs).caplength));
    let hciversion = read_volatile(addr_of!((*cap_regs).hciversion));
    let hcsparams1 = read_volatile(addr_of!((*cap_regs).hcsparams1));
    let hccparams1 = read_volatile(addr_of!((*cap_regs).hccparams1));
    
    kprintln!("USB: xHCI Version: {:x}.{:x} (Raw: {:#x}, CapLen: {})", 
        hciversion >> 8, hciversion & 0xFF, hciversion, caplength);
    
    let op_regs_offset = caplength as usize;
    let op_regs = mmio_base.add(op_regs_offset) as *mut OperationalRegisters;
    
    // 5. 解析参数
    let max_slots = (hcsparams1 & 0xFF) as u8;
    let max_ints = ((hcsparams1 >> 8) & 0x7FF) as u16;
    let max_ports = ((hcsparams1 >> 24) & 0xFF) as u8;
    
    kprintln!("USB: Max Slots: {}, Max Ports: {}, Max Ints: {}", 
        max_slots, max_ports, max_ints);

    // Check Context Size (CSZ, bit 2 of HCCPARAMS1)
    let context_size_64 = (hccparams1 & (1 << 2)) != 0;
    if context_size_64 {
        kprintln!("USB: Context Size is 64 bytes");
    } else {
        kprintln!("USB: Context Size is 32 bytes");
    }

    // 计算 Runtime Registers 和 Doorbell Registers 地址
    let dboff = read_volatile(addr_of!((*cap_regs).dboff)) & !0x3;
    let rtsoff = read_volatile(addr_of!((*cap_regs).rtsoff)) & !0x1F;
    
    let db_regs = mmio_base.add(dboff as usize) as *mut DoorbellRegisters;
    let rt_regs = mmio_base.add(rtsoff as usize) as *mut RuntimeRegisters;
    
    // 6. 重置控制器
    reset_controller(op_regs);

    // 启用 MSI/MSI-X 中断
    if pci::enable_msi(addr, IRQ_XHCI) {
        ok!("USB: xHCI MSI Enabled (Vector {})", IRQ_XHCI);
    } else {
        // Try MSI-X
        // Note: mmio_base corresponds to BAR0, which covers most xHCI MSI-X tables
        if pci::msix::enable_msix(addr, mmio_base, IRQ_XHCI) {
            ok!("USB: xHCI MSI-X Enabled (Vector {})", IRQ_XHCI);
        } else {
            warn!("USB: Failed to enable MSI/MSI-X for xHCI");
        }
    }
    
    // 7. 构造控制器实例
    let mut controller = XhciController {
        pci_addr: addr,
        mmio_base,
        mmio_size,
        cap_regs,
        op_regs,
        rt_regs,
        db_regs,
        max_slots,
        max_ports,
        context_size_64,
        dcbaap_phys: 0,
        dcbaap_virt: core::ptr::null_mut(),
        cmd_ring_phys: 0,
        cmd_ring_virt: core::ptr::null_mut(),
        cmd_ring_enqueue_idx: 0,
        cmd_ring_cycle_state: 1, // Producer Cycle State starts at 1
        event_ring_phys: 0,
        event_ring_virt: core::ptr::null_mut(),
        event_ring_dequeue_idx: 0,
        event_ring_cycle_state: 1, // Consumer Cycle State starts at 1
        erst_phys: 0,
        slots: [None; 32],
    };
    
    // 8. 初始化内存结构
    if !init_memory_structures(&mut controller) {
        kprintln!("USB: Failed to initialize xHCI memory structures");
        return;
    }
    
    // 9. 启动控制器
    start_controller(&mut controller);
    
    // 10. 检查端口状态
    check_ports(&mut controller);
    
    // Enable Global Interrupts (USBCMD_INTE)
    // Interrupts are enabled at Interrupter level in init_memory_structures (IMAN_IE)
    // Now we enable them at Controller level.
    let usbcmd = &mut (*controller.op_regs).usbcmd;
    let mut cmd = read_volatile(usbcmd);
    cmd |= USBCMD_INTE; // Enable Interrupter
    write_volatile(usbcmd, cmd);
    ok!("USB: xHCI Interrupts Enabled (USBCMD_INTE)");
    
    // 保存控制器实例
    XHCI_CONTROLLER = Some(controller);
    
    ok!("USB: xHCI Controller Initialized");
}

unsafe fn check_ports(xhci: &mut XhciController) {
    let op_base = xhci.op_regs as *mut u8;
    // Port Register Set is at offset 0x400 from Operational Registers Base
    let port_base = op_base.add(0x400); 
    
    info!("USB: Checking {} ports...", xhci.max_ports);

    for i in 0..xhci.max_ports {
        let port_sc_addr = port_base.add((i as usize) * 0x10) as *mut u32;
        let port_sc = read_volatile(port_sc_addr);
        
        if (port_sc & PORTSC_CCS) != 0 {
                    let speed = (port_sc >> 10) & 0xF;
                    info!("USB: Port {} Connected (Status: {:#x}, Speed: {})", i + 1, port_sc, speed);
                    
                    // Reset Port to enable it
                    if (port_sc & PORTSC_PED) == 0 {
                        debug!("USB: Resetting Port {}...", i + 1);
                        let mut new_sc = port_sc | PORTSC_PR;
                        // 清除状态位 (Write 1 to Clear)
                        new_sc &= !(PORTSC_CSC | PORTSC_PRC); // 不要写 1 到这些位，否则会清除它们? 
                        // Wait, spec says RW1C. If we write 1, we clear them.
                        // We want to SET PR.
                        // Ideally we read, mask off RW1C bits, set PR, write back.
                        // RW1C bits: CSC (17), PESC (18), WRC (19), OC (20), PRC (21), PLC (22), CEC (23)
                        let change_bits = 0x00FE0000; // Bits 17-23
                        new_sc = (port_sc & !change_bits) | PORTSC_PR;
                        
                        write_volatile(port_sc_addr, new_sc);
                        
                        // Wait for PRC
                        let mut retries = 100;
                        while retries > 0 {
                            let sc = read_volatile(port_sc_addr);
                            if (sc & PORTSC_PRC) != 0 {
                                // Clear PRC
                                write_volatile(port_sc_addr, (sc & !change_bits) | PORTSC_PRC);
                                ok!("USB: Port {} Reset Complete (Status: {:#x})", i + 1, sc);
                                
                                // Try to enable slot for this port
                                if let Some(slot_id) = xhci.enable_slot() {
                                    info!("USB: Port {} Slot ID: {}", i + 1, slot_id);
                                    // Next step: Address Device
                                    if xhci.address_device(slot_id, (i + 1) as u8, speed) {
                                        xhci.configure_device(slot_id);
                                    }
                                }
                                
                                break;
                            }
                            // Simple delay
                            for _ in 0..10000 { core::hint::spin_loop(); }
                            retries -= 1;
                        }
                    }
                } else {
             // kprintln!("USB: Port {} Disconnected (Status: {:#x})", i + 1, port_sc);
        }
    }
}

unsafe fn alloc_dma_zeroed() -> Option<(u64, *mut u8)> {
    let page = alloc_pages(0, GFP_KERNEL_ZERO)?;
    let pfn = page_to_pfn(page);
    let phys = pfn * PAGE_SIZE;
    let virt = (phys + DIRECT_MAP_OFFSET) as *mut u8;
    
    // 显式清零
    core::ptr::write_bytes(virt, 0, PAGE_SIZE as usize);
    
    Some((phys, virt))
}

unsafe fn init_memory_structures(xhci: &mut XhciController) -> bool {
    // 1. 设置 Max Slots Enabled (CONFIG 寄存器)
    let config = &mut (*xhci.op_regs).config;
    write_volatile(config, xhci.max_slots as u32);

    // 2. 分配 DCBAA (Device Context Base Address Array)
    let (dcbaap_phys, dcbaap_virt) = match alloc_dma_zeroed() {
        Some(x) => x,
        None => return false,
    };
    xhci.dcbaap_phys = dcbaap_phys;
    xhci.dcbaap_virt = dcbaap_virt as *mut u64;
    
    // 写入 DCBAAP 寄存器
    write_volatile(&mut (*xhci.op_regs).dcbaap, dcbaap_phys);

    // 3. 分配 Command Ring
    let (cmd_ring_phys, cmd_ring_virt) = match alloc_dma_zeroed() {
        Some(x) => x,
        None => return false,
    };
    xhci.cmd_ring_phys = cmd_ring_phys;
    xhci.cmd_ring_virt = cmd_ring_virt as *mut Trb;
    
    // 写入 CRCR 寄存器 (RCS = 1)
    write_volatile(&mut (*xhci.op_regs).crcr, cmd_ring_phys | CRCR_RCS);

    // 4. 分配 Event Ring 和 ERST
    // 4.1 Event Ring Segment
    let (er_seg_phys, er_seg_virt) = match alloc_dma_zeroed() {
        Some(x) => x,
        None => return false,
    };
    xhci.event_ring_phys = er_seg_phys;
    xhci.event_ring_virt = er_seg_virt as *mut Trb;

    // 4.2 ERST (Segment Table)
    let (erst_phys, erst_virt) = match alloc_dma_zeroed() {
        Some(x) => x,
        None => return false,
    };
    xhci.erst_phys = erst_phys;

    // 填充 ERST Entry 0
    let erst_entry = &mut *(erst_virt as *mut EventRingSegmentTableEntry);
    erst_entry.base_addr = er_seg_phys;
    erst_entry.size = 256; // 4096 / 16 = 256 TRBs
    erst_entry.reserved = 0;
    erst_entry.reserved2 = 0;

    // 5. 设置 Interrupter 0
    // Runtime Registers -> Interrupter Register Set 0
    let ir_set = addr_of_mut!((*xhci.rt_regs).irs) as *mut InterrupterRegisters;
    let ir0 = &mut *ir_set; // First interrupter

    // 设置 ERSTSZ (Table Size = 1)
    write_volatile(&mut ir0.erstsz, 1);
    
    // 设置 ERDP (Dequeue Pointer)
    write_volatile(&mut ir0.erdp, er_seg_phys);
    
    // 设置 ERSTBA (Table Base Address)
    write_volatile(&mut ir0.erstba, erst_phys);
    
    // 启用中断 (IMAN)
    // Bit 0: Interrupt Pending (RW1C) - Write 1 to clear
    // Bit 1: Interrupt Enable (RW) - Set to 1
    let iman = read_volatile(&ir0.iman);
    write_volatile(&mut ir0.iman, iman | 0x02 | 0x01); // Enable + Clear Pending

    true
}

unsafe fn start_controller(xhci: &mut XhciController) {
    let usbcmd = &mut (*xhci.op_regs).usbcmd;
    let mut cmd = read_volatile(usbcmd);
    cmd |= USBCMD_RUN_STOP;
    write_volatile(usbcmd, cmd);
    
    kprintln!("USB: xHCI Controller Started");
}

unsafe fn reset_controller(op_regs: *mut OperationalRegisters) {
    let usbcmd = &mut (*op_regs).usbcmd;
    let usbsts = &mut (*op_regs).usbsts;
    
    // 1. 停止控制器 (Clear RUN/STOP bit)
    let mut cmd = read_volatile(usbcmd);
    cmd &= !USBCMD_RUN_STOP;
    write_volatile(usbcmd, cmd);
    
    // 等待 HCH (Host Controller Halted)
    kprintln!("USB: Waiting for xHCI halt...");
    let mut timeout = 1000;
    while (read_volatile(usbsts) & USBSTS_HCH) == 0 {
        if timeout == 0 {
            kprintln!("USB: Timeout waiting for xHCI halt");
            break;
        }
        // TODO: Use better delay
        for _ in 0..10000 { core::hint::spin_loop(); }
        timeout -= 1;
    }
    
    // 2. 重置控制器 (Set RESET bit)
    debug!("USB: Resetting xHCI...");
    cmd = read_volatile(usbcmd);
    cmd |= USBCMD_RESET;
    write_volatile(usbcmd, cmd);
    
    // 等待 RESET 位清除
    timeout = 1000;
    while (read_volatile(usbcmd) & USBCMD_RESET) != 0 {
        if timeout == 0 {
            warn!("USB: Timeout waiting for xHCI reset");
            break;
        }
        for _ in 0..10000 { core::hint::spin_loop(); }
        timeout -= 1;
    }
    
    // 3. 等待 CNR (Controller Not Ready) 清除
    timeout = 1000;
    while (read_volatile(usbsts) & USBSTS_CNR) != 0 {
        if timeout == 0 {
            warn!("USB: Timeout waiting for xHCI ready");
            break;
        }
        for _ in 0..10000 { core::hint::spin_loop(); }
        timeout -= 1;
    }
    
    debug!("USB: xHCI Reset Complete");
}

// Function moved to pci/msix.rs

/// xHCI 中断处理函数
pub fn handle_interrupt() {
    unsafe {
        if let Some(xhci) = &mut *core::ptr::addr_of_mut!(XHCI_CONTROLLER) {
            // 1. Clear IP bit (Interrupt Pending) in IMAN (Write 1 to clear)
            let ir_set = addr_of_mut!((*xhci.rt_regs).irs) as *mut InterrupterRegisters;
            let ir0 = &mut *ir_set;
            let iman = read_volatile(&ir0.iman);
            if (iman & 1) != 0 {
                write_volatile(&mut ir0.iman, iman | 1);
            }
            
            // 2. Process Event Ring
            // Loop until no more events
            while let Some(trb) = xhci.poll_event() {
                 let trb_type = (trb.control >> 10) & 0x3F;
                 if trb_type == TRB_TYPE_TRANSFER_EVENT {
                     xhci.handle_transfer_event(&trb);
                 } else {
                     // Log other events?
                     // debug!("USB: IRQ Event Type {}", trb_type);
                 }
            }
        }
    }
}
