//! VirtIO Block Device Driver
//!
//! Supports both transitional (0x1001) and modern (0x1042) VirtIO block devices.

use crate::config::PAGE_SIZE;
use crate::drivers::pci::{self, driver::{PciDeviceId, PciDriver, ProbeResult}, PciAddress, PciHeader};
use crate::mm::page::buddy::alloc_pages;
use crate::mm::page::zone::{GfpFlags, GFP_DMA32};
use crate::mm::vm::layout::{page_align_up, phys_to_virt, PAGE_SIZE as PAGE_SIZE_U64};
use crate::mm::vmalloc::ioremap;
use crate::sync::IrqSpinLock;
use crate::{diag, error, info, ok, warn};
use alloc::boxed::Box;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use super::{BlockDevice, BlockError};

const VIRTIO_BLK_SECTOR_SIZE: u32 = 512;
const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;

const VIRTIO_PCI_VENDOR_ID: u16 = 0x1AF4;

// Device IDs
const VIRTIO_PCI_DEVICE_TRANSITIONAL_BLOCK: u16 = 0x1001;
const VIRTIO_PCI_DEVICE_MODERN_BLOCK: u16 = 0x1042;

// VirtIO PCI Capability IDs
const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;

// In feature word 1, bit 0 = VIRTIO_F_VERSION_1
const VIRTIO_F_VERSION_1: u32 = 1;

const VIRTIO_BLK_F_RO: u32 = 1 << 5;
const VIRTIO_BLK_F_FLUSH: u32 = 1 << 9;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

#[repr(C)]
struct VirtioBlkConfig {
    capacity: u64,
    size_max: u32,
    seg_max: u32,
    geometry: VirtioBlkGeometry,
    blk_size: u32,
    topology: VirtioBlkTopology,
    writeback: u8,
    _unused: [u8; 3],
}

#[repr(C)]
struct VirtioBlkGeometry {
    cylinders: u16,
    heads: u8,
    sectors: u8,
}

#[repr(C)]
struct VirtioBlkTopology {
    physical_block_exp: u8,
    alignment_offset: u8,
    min_io_size: u16,
    opt_io_size: u32,
}

#[repr(C)]
struct VirtioBlkReqHeader {
    type_: u32,
    reserved: u32,
    sector: u64,
}

#[repr(C)]
struct VirtioPciCommonCfg {
    device_feature_select: u32,
    device_feature: u32,
    driver_feature_select: u32,
    driver_feature: u32,
    msix_config: u16,
    num_queues: u16,
    device_status: u8,
    config_generation: u8,
    queue_select: u16,
    queue_size: u16,
    queue_msix_vector: u16,
    queue_enable: u16,
    queue_notify_off: u16,
    queue_desc: u64,
    queue_driver: u64,
    queue_device: u64,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, align(2))]
struct VirtqAvail {
    flags: u16,
    idx: u16,
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4))]
struct VirtqUsed {
    flags: u16,
    idx: u16,
}

struct DmaBuffer {
    virt: *mut u8,
    phys: u64,
    size: usize,
}

impl DmaBuffer {
    fn new(size: usize) -> Option<Self> {
        let aligned_size = page_align_up(size as u64) as usize;
        let page_size = PAGE_SIZE as usize;
        let pages = (aligned_size / page_size).max(1);
        let pages_pow2 = pages.next_power_of_two();
        let order = pages_pow2.trailing_zeros() as usize;

        let gfp = GfpFlags::new(GFP_DMA32.bits() | GfpFlags::ZERO);
        let page = alloc_pages(order.max(0), gfp)?;

        let pfn = crate::mm::page_to_pfn(page);
        let phys = pfn * PAGE_SIZE_U64;
        let virt = phys_to_virt(phys) as *mut u8;

        Some(Self { virt, phys, size: pages_pow2 * page_size })
    }

    fn as_ptr(&self) -> *mut u8 { self.virt }
    fn phys_addr(&self) -> u64 { self.phys }
}

/// VirtIO capability information
struct VirtioCap {
    bar: u8,
    offset: u32,
    length: u32,
    notify_off_multiplier: u32,
}

/// VirtIO block device
pub struct VirtioBlkDevice {
    common_cfg: *mut VirtioPciCommonCfg,
    config: *const VirtioBlkConfig,
    notify_base: *mut u8,
    notify_len: usize,
    notify_off_multiplier: u32,
    queue_notify: *mut u16,
    queue_size: u32,
    vq_mem: Option<DmaBuffer>,
    desc: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    used: *mut VirtqUsed,
    req_mem: Option<DmaBuffer>,
    data_mem: Option<DmaBuffer>,
    avail_idx: AtomicU16,
    block_count: u64,
    read_only: bool,
    initialized: AtomicBool,
}

unsafe impl Send for VirtioBlkDevice {}
unsafe impl Sync for VirtioBlkDevice {}

impl VirtioBlkDevice {
    pub const fn new() -> Self {
        Self {
            common_cfg: core::ptr::null_mut(),
            config: core::ptr::null(),
            notify_base: core::ptr::null_mut(),
            notify_len: 0,
            notify_off_multiplier: 0,
            queue_notify: core::ptr::null_mut(),
            queue_size: 0,
            vq_mem: None,
            desc: core::ptr::null_mut(),
            avail: core::ptr::null_mut(),
            used: core::ptr::null_mut(),
            req_mem: None,
            data_mem: None,
            avail_idx: AtomicU16::new(0),
            block_count: 0,
            read_only: false,
            initialized: AtomicBool::new(false),
        }
    }

    /// Read PCI capability byte at offset
    unsafe fn read_cap_byte(addr: PciAddress, offset: u8) -> u8 {
        let aligned_offset = offset & 0xFC;
        let shift = (offset & 0x3) * 8;
        let val = pci::read_config_32(addr, aligned_offset);
        ((val >> shift) & 0xFF) as u8
    }

    /// Read PCI capability dword at offset
    unsafe fn read_cap_dword(addr: PciAddress, offset: u8) -> u32 {
        pci::read_config_32(addr, offset & 0xFC)
    }

    fn find_capability(addr: PciAddress, cap_id: u8) -> Option<VirtioCap> {
        // PCI status bit 4 indicates capability list support.
        if (pci::read_header(addr).status & (1 << 4)) == 0 {
            diag!("[virtio] no PCI capabilities list");
            return None;
        }

        // Find PCI capabilities pointer
        let caps_ptr = (unsafe { pci::read_config_32(addr, 0x34) } as u8) & !0x3;
        if caps_ptr == 0 {
            diag!("[virtio] no PCI capabilities list");
            return None;
        }

        diag!("[virtio] scanning capabilities from {:#x}", caps_ptr);

        let mut offset = caps_ptr;
        let mut count = 0u8;
        while offset != 0 && count < 48 {
            // Read capability header
            let cap_vndr = unsafe { Self::read_cap_byte(addr, offset) };
            let cap_next = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(1)) } & !0x3;
            let cap_len = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(2)) };

            diag!("[virtio] cap at {:#x}: vndr={:#x} next={:#x} len={}", 
                  offset, cap_vndr, cap_next, cap_len);

            // Check if this is a Vendor-Specific capability (0x09)
            if cap_vndr == 0x09 {
                let cfg_type = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(3)) };
                diag!("[virtio] vendor cap type={}", cfg_type);

                if cfg_type == cap_id {
                    // Read BAR, offset, and length
                    let bar = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(4)) };
                    // offset is at offset+8, length at offset+12
                    let cap_offset = unsafe { Self::read_cap_dword(addr, offset.wrapping_add(8)) };
                    let length = unsafe { Self::read_cap_dword(addr, offset.wrapping_add(12)) };
                    let notify_off_multiplier = if cap_id == VIRTIO_PCI_CAP_NOTIFY_CFG && cap_len >= 20 {
                        unsafe { Self::read_cap_dword(addr, offset.wrapping_add(16)) }
                    } else {
                        0
                    };

                    diag!("[virtio] found cap {} at BAR{} offset {:#x} len {:#x}",
                          cap_id, bar, cap_offset, length);

                    return Some(VirtioCap {
                        bar,
                        offset: cap_offset,
                        length,
                        notify_off_multiplier,
                    });
                }
            }

            if cap_next == offset {
                warn!("[virtio] broken capability chain at {:#x}", offset);
                break;
            }
            offset = cap_next;
            count += 1;
        }

        diag!("[virtio] cap {} not found (scanned {} caps)", cap_id, count);
        None
    }

    fn read_bar_phys(addr: PciAddress, bar_index: u8) -> Result<u64, BlockError> {
        let bar_offset: u8 = (0x10 + (bar_index as u16 * 4)) as u8;
        let bar_lo = unsafe { pci::read_config_32(addr, bar_offset) };

        diag!("[virtio] BAR{} = {:#x}", bar_index, bar_lo);

        // Check if it's I/O space (bit 0)
        if (bar_lo & 0x1) != 0 {
            warn!("[virtio] BAR{} is I/O space (legacy mode)", bar_index);
            return Err(BlockError::Unsupported);
        }

        // Check if BAR is not implemented
        if bar_lo == 0 || bar_lo == 0xFFFFFFF0 {
            warn!("[virtio] BAR{} not implemented", bar_index);
            return Err(BlockError::IoError);
        }

        // Parse memory BAR type
        let mem_type = (bar_lo >> 1) & 0x3;
        if mem_type == 1 || mem_type == 3 {
            warn!("[virtio] BAR{} unsupported type {}", bar_index, mem_type);
            return Err(BlockError::Unsupported);
        }

        let mut mmio_phys = (bar_lo & 0xFFFF_FFF0) as u64;
        if mem_type == 2 {
            // 64-bit BAR
            if bar_index >= 5 {
                warn!("[virtio] BAR{} 64-bit pair is out of range", bar_index);
                return Err(BlockError::Unsupported);
            }
            let bar1 = unsafe { pci::read_config_32(addr, bar_offset + 4) };
            mmio_phys |= (bar1 as u64) << 32;
            diag!(
                "[virtio] BAR{} low={:#x} high={:#x} phys={:#x}",
                bar_index,
                bar_lo,
                bar1,
                mmio_phys
            );
        }

        if mmio_phys == 0 {
            warn!("[virtio] BAR{} physical address is zero", bar_index);
            return Err(BlockError::IoError);
        }

        Ok(mmio_phys)
    }

    fn map_capability(addr: PciAddress, cap: &VirtioCap, min_len: usize) -> Result<*mut u8, BlockError> {
        let bar_phys = Self::read_bar_phys(addr, cap.bar)?;
        let map_phys = bar_phys
            .checked_add(cap.offset as u64)
            .ok_or(BlockError::IoError)?;
        let map_len = core::cmp::max(cap.length as usize, min_len);
        if map_len == 0 {
            return Err(BlockError::IoError);
        }

        let mmio_virt = ioremap(map_phys, map_len);
        if mmio_virt.is_null() {
            error!(
                "[virtio] failed to map BAR{} offset {:#x} len {:#x}",
                cap.bar,
                cap.offset,
                map_len
            );
            return Err(BlockError::IoError);
        }

        diag!(
            "[virtio] BAR{} mapped phys={:#x} len={:#x} -> {:#x}",
            cap.bar,
            map_phys,
            map_len,
            mmio_virt as u64
        );
        Ok(mmio_virt)
    }

    fn init_modern(&mut self, addr: PciAddress, _header: &PciHeader) -> Result<(), BlockError> {
        diag!("[virtio] initializing at {:?}", addr);

        // Enable Memory Space + Bus Master.
        let cmd = unsafe { pci::read_config_32(addr, 0x04) };
        let new_cmd = (cmd & 0xFFFF_0000) | ((cmd as u16 | 0x06) as u32);
        if new_cmd != cmd {
            unsafe { pci::write_config_32(addr, 0x04, new_cmd) };
        }

        let common_cap = if let Some(cap) = Self::find_capability(addr, VIRTIO_PCI_CAP_COMMON_CFG) {
            cap
        } else {
            warn!("[virtio-blk] no MMIO capabilities, device may be legacy-only");
            return Err(BlockError::Unsupported);
        };
        let notify_cap = if let Some(cap) = Self::find_capability(addr, VIRTIO_PCI_CAP_NOTIFY_CFG) {
            cap
        } else {
            warn!("[virtio-blk] missing notify capability");
            return Err(BlockError::Unsupported);
        };
        let device_cap = if let Some(cap) = Self::find_capability(addr, VIRTIO_PCI_CAP_DEVICE_CFG) {
            cap
        } else {
            warn!("[virtio-blk] missing device config capability");
            return Err(BlockError::Unsupported);
        };

        info!("[virtio-blk] using PCI modern capabilities");
        self.common_cfg = Self::map_capability(addr, &common_cap, core::mem::size_of::<VirtioPciCommonCfg>())?
            as *mut VirtioPciCommonCfg;
        self.config = Self::map_capability(addr, &device_cap, core::mem::size_of::<VirtioBlkConfig>())?
            as *const VirtioBlkConfig;
        self.notify_base = Self::map_capability(addr, &notify_cap, 2)?;
        self.notify_len = core::cmp::max(notify_cap.length as usize, 2);
        self.notify_off_multiplier = notify_cap.notify_off_multiplier;
        if self.notify_off_multiplier == 0 {
            warn!("[virtio-blk] invalid notify_off_multiplier=0");
            return Err(BlockError::Unsupported);
        }

        self.init_device()
    }

    fn set_status(&self, status: u8) {
        unsafe { write_volatile(&mut (*self.common_cfg).device_status, status) };
    }

    fn get_status(&self) -> u8 {
        unsafe { read_volatile(&(*self.common_cfg).device_status) }
    }

    fn read_device_features(&self, select: u32) -> u32 {
        unsafe {
            write_volatile(&mut (*self.common_cfg).device_feature_select, select);
            read_volatile(&(*self.common_cfg).device_feature)
        }
    }

    fn write_driver_features(&self, select: u32, value: u32) {
        unsafe {
            write_volatile(&mut (*self.common_cfg).driver_feature_select, select);
            write_volatile(&mut (*self.common_cfg).driver_feature, value);
        }
    }

    fn init_device(&mut self) -> Result<(), BlockError> {
        if self.common_cfg.is_null() || self.config.is_null() || self.notify_base.is_null() {
            return Err(BlockError::NotReady);
        }

        self.set_status(0);
        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE);
        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

        let features_lo = self.read_device_features(0);
        let features_hi = self.read_device_features(1);
        if (features_hi & VIRTIO_F_VERSION_1) == 0 {
            warn!("[virtio] device does not advertise VIRTIO_F_VERSION_1");
            return Err(BlockError::Unsupported);
        }
        self.read_only = (features_lo & VIRTIO_BLK_F_RO) != 0;

        let mut driver_features_lo = 0u32;
        if (features_lo & VIRTIO_BLK_F_FLUSH) != 0 {
            driver_features_lo |= VIRTIO_BLK_F_FLUSH;
        }
        self.write_driver_features(0, driver_features_lo);
        self.write_driver_features(1, VIRTIO_F_VERSION_1);

        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);
        let status = self.get_status();
        if (status & VIRTIO_STATUS_FEATURES_OK) == 0 {
            error!("[virtio] feature negotiation failed");
            return Err(BlockError::Unsupported);
        }

        self.setup_virtqueue()?;
        self.set_status(
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );

        self.block_count = unsafe { read_volatile(&(*self.config).capacity) };
        self.initialized.store(true, Ordering::SeqCst);

        let size_mb = self.block_count * VIRTIO_BLK_SECTOR_SIZE as u64 / 1024 / 1024;
        ok!("[virtio-blk] {} MB", size_mb);

        Ok(())
    }

    fn setup_virtqueue(&mut self) -> Result<(), BlockError> {
        unsafe {
            write_volatile(&mut (*self.common_cfg).queue_select, 0);
        }

        self.queue_size = unsafe { read_volatile(&(*self.common_cfg).queue_size) as u32 };
        if self.queue_size == 0 {
            error!("[virtio] queue 0 unavailable");
            return Err(BlockError::IoError);
        }
        self.queue_size = self.queue_size.min(256);
        unsafe {
            write_volatile(&mut (*self.common_cfg).queue_size, self.queue_size as u16);
        }

        let n = self.queue_size as usize;
        let desc_size = n * core::mem::size_of::<VirtqDesc>();
        let avail_size = 4 + n * 2;
        let used_size = page_align_up((4 + n * core::mem::size_of::<VirtqUsedElem>()) as u64) as usize;
        let total = page_align_up((desc_size + avail_size + used_size) as u64) as usize;

        let vq_mem = DmaBuffer::new(total).ok_or(BlockError::IoError)?;

        let base = vq_mem.as_ptr();
        let desc_phys = vq_mem.phys_addr();

        self.desc = base as *mut VirtqDesc;
        self.avail = unsafe { base.add(desc_size) as *mut VirtqAvail };
        let avail_phys = desc_phys + desc_size as u64;

        let used_offset = page_align_up((desc_size + avail_size) as u64) as usize;
        self.used = unsafe { base.add(used_offset) as *mut VirtqUsed };
        let used_phys = desc_phys + used_offset as u64;

        unsafe {
            core::ptr::write_bytes(self.desc, 0, n);
            write_volatile(&mut (*self.avail).flags, 0);
            write_volatile(&mut (*self.avail).idx, 0);
            write_volatile(&mut (*self.used).flags, 0);
            write_volatile(&mut (*self.used).idx, 0);
        }

        self.req_mem = DmaBuffer::new(core::mem::size_of::<VirtioBlkReqHeader>() + 8).into();
        self.data_mem = DmaBuffer::new(VIRTIO_BLK_SECTOR_SIZE as usize).into();

        let notify_off = unsafe { read_volatile(&(*self.common_cfg).queue_notify_off) } as usize;
        let notify_offset = notify_off
            .checked_mul(self.notify_off_multiplier as usize)
            .ok_or(BlockError::IoError)?;
        if notify_offset + core::mem::size_of::<u16>() > self.notify_len {
            error!(
                "[virtio] notify offset out of range (off={:#x}, len={:#x})",
                notify_offset,
                self.notify_len
            );
            return Err(BlockError::IoError);
        }
        self.queue_notify = unsafe { self.notify_base.add(notify_offset) as *mut u16 };
        crate::mm::vmalloc::set_vmalloc_watch_page(self.queue_notify as u64);
        if !crate::mm::vmalloc::ensure_vmalloc_page_mapped_in_current(self.queue_notify as u64) {
            error!(
                "[virtio] queue notify mapping missing during setup: queue_notify={:#x} off={:#x} len={:#x}",
                self.queue_notify as u64,
                notify_offset,
                self.notify_len
            );
            return Err(BlockError::IoError);
        }
        if crate::config::DEBUG_VERBOSE {
            if let Some((cur_root, init_root, cur_phys, init_phys)) =
                crate::mm::vmalloc::vmalloc_mapping_state(self.queue_notify as u64)
            {
                diag!(
                    "[virtio] queue_notify state after setup: notify={:#x} cur_root={:#x} init_root={:#x} cur_phys={:#x?} init_phys={:#x?}",
                    self.queue_notify as u64,
                    cur_root,
                    init_root,
                    cur_phys,
                    init_phys
                );
            }
        }

        unsafe {
            write_volatile(&mut (*self.common_cfg).queue_desc, desc_phys);
            write_volatile(&mut (*self.common_cfg).queue_driver, avail_phys);
            write_volatile(&mut (*self.common_cfg).queue_device, used_phys);
            write_volatile(&mut (*self.common_cfg).queue_enable, 1);
        }

        self.vq_mem = Some(vq_mem);
        self.avail_idx.store(0, Ordering::SeqCst);

        Ok(())
    }

    fn do_io(&self, write_op: bool, lba: u64, data: *mut u8, len: usize) -> Result<(), BlockError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(BlockError::NotReady);
        }

        let req_mem = self.req_mem.as_ref().ok_or(BlockError::NotReady)?;
        let data_mem = self.data_mem.as_ref().ok_or(BlockError::NotReady)?;

        let req = req_mem.as_ptr() as *mut VirtioBlkReqHeader;
        unsafe {
            (*req).type_ = if write_op { VIRTIO_BLK_T_OUT } else { VIRTIO_BLK_T_IN };
            (*req).reserved = 0;
            (*req).sector = lba;
        }

        if write_op {
            unsafe { core::ptr::copy_nonoverlapping(data, data_mem.as_ptr(), len); }
        }

        let desc = self.desc;
        let resp_phys = req_mem.phys_addr() + core::mem::size_of::<VirtioBlkReqHeader>() as u64;

        unsafe {
            (*desc.add(0)).addr = req_mem.phys_addr();
            (*desc.add(0)).len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;
            (*desc.add(0)).flags = VIRTQ_DESC_F_NEXT;
            (*desc.add(0)).next = 1;

            (*desc.add(1)).addr = data_mem.phys_addr();
            (*desc.add(1)).len = len as u32;
            (*desc.add(1)).flags = VIRTQ_DESC_F_NEXT | if write_op { 0 } else { VIRTQ_DESC_F_WRITE };
            (*desc.add(1)).next = 2;

            (*desc.add(2)).addr = resp_phys;
            (*desc.add(2)).len = 1;
            (*desc.add(2)).flags = VIRTQ_DESC_F_WRITE;
            (*desc.add(2)).next = 0;
        }

        let avail = self.avail;
        let idx = self.avail_idx.fetch_add(1, Ordering::SeqCst);
        let ring_idx = (idx % self.queue_size as u16) as usize;
        unsafe {
            let ring = (avail as *mut u8).add(4) as *mut u16;
            *ring.add(ring_idx) = 0;
            core::sync::atomic::fence(Ordering::SeqCst);
            write_volatile(&mut (*avail).idx, idx.wrapping_add(1));
        }

        if self.queue_notify.is_null() {
            return Err(BlockError::NotReady);
        }
        if crate::config::DEBUG_VERBOSE {
            if let Some((cur_root, init_root, cur_phys, init_phys)) =
                crate::mm::vmalloc::vmalloc_mapping_state(self.queue_notify as u64)
            {
                if cur_phys.is_none() || init_phys.is_none() {
                    diag!(
                        "[virtio] queue_notify unmapped before doorbell: notify={:#x} cur_root={:#x} init_root={:#x} cur_phys={:#x?} init_phys={:#x?}",
                        self.queue_notify as u64,
                        cur_root,
                        init_root,
                        cur_phys,
                        init_phys
                    );
                }
            }
        }
        unsafe { write_volatile(self.queue_notify, 0); }

        let used = self.used;
        let target_used = idx.wrapping_add(1);
        let mut completed = false;
        for _ in 0..1_000_000 {
            let used_idx = unsafe { read_volatile(&(*used).idx) };
            if used_idx == target_used {
                completed = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !completed {
            let used_idx = unsafe { read_volatile(&(*used).idx) };
            error!(
                "[virtio] queue timeout op={} lba={} used_idx={} target={}",
                if write_op { "write" } else { "read" },
                lba,
                used_idx,
                target_used
            );
            return Err(BlockError::Timeout);
        }

        let status = unsafe {
            read_volatile((req_mem.as_ptr().add(core::mem::size_of::<VirtioBlkReqHeader>()) as *const u8))
        };

        match status {
            VIRTIO_BLK_S_OK => {
                if !write_op {
                    unsafe { core::ptr::copy_nonoverlapping(data_mem.as_ptr(), data, len); }
                }
                Ok(())
            }
            VIRTIO_BLK_S_IOERR => Err(BlockError::IoError),
            other => {
                error!(
                    "[virtio] request failed op={} lba={} status={:#x}",
                    if write_op { "write" } else { "read" },
                    lba,
                    other
                );
                Err(BlockError::Unsupported)
            }
        }
    }
}

impl BlockDevice for VirtioBlkDevice {
    fn block_size(&self) -> u32 { VIRTIO_BLK_SECTOR_SIZE }
    fn block_count(&self) -> u64 { self.block_count }
    fn name(&self) -> &str { "virtio-blk" }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() < VIRTIO_BLK_SECTOR_SIZE as usize { return Err(BlockError::InvalidBufferSize); }
        if lba >= self.block_count { return Err(BlockError::InvalidAddress); }
        self.do_io(false, lba, buf.as_mut_ptr(), VIRTIO_BLK_SECTOR_SIZE as usize)
    }

    fn write_block(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        if self.read_only { return Err(BlockError::WriteProtected); }
        if buf.len() < VIRTIO_BLK_SECTOR_SIZE as usize { return Err(BlockError::InvalidBufferSize); }
        if lba >= self.block_count { return Err(BlockError::InvalidAddress); }
        self.do_io(true, lba, buf.as_ptr() as *mut u8, VIRTIO_BLK_SECTOR_SIZE as usize)
    }

    fn flush(&self) -> Result<(), BlockError> {
        if !self.initialized.load(Ordering::SeqCst) { return Err(BlockError::NotReady); }
        Ok(())
    }

    fn is_read_only(&self) -> bool { self.read_only }
}

static VIRTIO_BLK: IrqSpinLock<VirtioBlkDevice> = IrqSpinLock::new(VirtioBlkDevice::new());

struct VirtioBlkDriver;

impl PciDriver for VirtioBlkDriver {
    fn name(&self) -> &'static str { "virtio-blk" }

    fn supported_ids(&self) -> &[PciDeviceId] {
        static IDS: [PciDeviceId; 2] = [
            PciDeviceId::vendor_device(VIRTIO_PCI_VENDOR_ID, VIRTIO_PCI_DEVICE_TRANSITIONAL_BLOCK),
            PciDeviceId::vendor_device(VIRTIO_PCI_VENDOR_ID, VIRTIO_PCI_DEVICE_MODERN_BLOCK),
        ];
        &IDS
    }

    fn probe(&self, addr: PciAddress, header: &PciHeader) -> ProbeResult {
        let mut dev = VIRTIO_BLK.lock();
        match dev.init_modern(addr, header) {
            Ok(()) => ProbeResult::Claimed,
            Err(BlockError::Unsupported) => {
                warn!("[virtio-blk] unsupported transport/features");
                ProbeResult::Unsupported
            }
            Err(e) => {
                error!("[virtio-blk] init failed ({:?})", e);
                ProbeResult::Error("init failed")
            }
        }
    }
}

pub fn register() {
    pci::register_driver(Box::new(VirtioBlkDriver));
}

pub fn get_device() -> Option<&'static VirtioBlkDevice> {
    let dev = VIRTIO_BLK.lock();
    if dev.initialized.load(Ordering::SeqCst) {
        Some(unsafe { &*(&*dev as *const VirtioBlkDevice) })
    } else {
        None
    }
}

pub fn init() {
    register();
}
