//! VirtIO SCSI block driver (minimal LUN0 read-only path).

use crate::config::PAGE_SIZE;
use crate::drivers::pci::{
    self, PciAddress, PciHeader,
    driver::{PciDeviceId, PciDriver, ProbeResult},
};
use crate::mm::page::buddy::alloc_pages;
use crate::mm::page::zone::{GFP_DMA32, GfpFlags};
use crate::mm::vm::layout::{PAGE_SIZE as PAGE_SIZE_U64, page_align_up, phys_to_virt};
use crate::mm::vmalloc::ioremap;
use crate::sync::IrqSpinLock;
use crate::{diag, error, info, ok, warn};
use alloc::boxed::Box;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use super::{BlockDevice, BlockError};

const VIRTIO_SCSI_SECTOR_SIZE_DEFAULT: u32 = 512;
const VIRTIO_SCSI_REQUESTQ: u16 = 2;

const VIRTIO_PCI_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_PCI_DEVICE_TRANSITIONAL_SCSI: u16 = 0x1004;
const VIRTIO_PCI_DEVICE_MODERN_SCSI: u16 = 0x1048;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_F_VERSION_1: u32 = 1;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

const SCSI_CMD_INQUIRY: u8 = 0x12;
const SCSI_CMD_READ_CAPACITY10: u8 = 0x25;
const SCSI_CMD_READ10: u8 = 0x28;

const SCSI_STATUS_GOOD: u8 = 0x00;
const VIRTIO_SCSI_S_OK: u8 = 0;

const INQUIRY_DATA_LEN: usize = 36;
const READ_CAPACITY10_DATA_LEN: usize = 8;
const MAX_CDB_SIZE: usize = 32;
const MAX_SENSE_SIZE: usize = 96;
const DATA_BUFFER_SIZE: usize = 4096;
const LUN0: [u8; 8] = [0x01, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00];

#[repr(C)]
struct VirtioScsiConfig {
    num_queues: u32,
    seg_max: u32,
    max_sectors: u32,
    cmd_per_lun: u32,
    event_info_size: u32,
    sense_size: u32,
    cdb_size: u32,
    max_channel: u16,
    max_target: u16,
    max_lun: u32,
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

        Some(Self {
            virt,
            phys,
            size: pages_pow2 * page_size,
        })
    }

    fn as_ptr(&self) -> *mut u8 {
        self.virt
    }

    fn phys_addr(&self) -> u64 {
        self.phys
    }
}

struct VirtioCap {
    bar: u8,
    offset: u32,
    length: u32,
    notify_off_multiplier: u32,
}

#[repr(C)]
struct VirtioScsiReq {
    lun: [u8; 8],
    id: u64,
    task_attr: u8,
    prio: u8,
    crn: u8,
    cdb: [u8; MAX_CDB_SIZE],
}

#[repr(C)]
struct VirtioScsiResp {
    sense_len: u32,
    residual: u32,
    status_qualifier: u16,
    status: u8,
    response: u8,
    sense: [u8; MAX_SENSE_SIZE],
}

pub struct VirtioScsiDevice {
    common_cfg: *mut VirtioPciCommonCfg,
    config: *const VirtioScsiConfig,
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
    resp_mem: Option<DmaBuffer>,
    data_mem: Option<DmaBuffer>,
    avail_idx: AtomicU16,
    io_lock: IrqSpinLock<()>,
    block_size: u32,
    block_count: u64,
    initialized: AtomicBool,
}

unsafe impl Send for VirtioScsiDevice {}
unsafe impl Sync for VirtioScsiDevice {}

impl VirtioScsiDevice {
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
            resp_mem: None,
            data_mem: None,
            avail_idx: AtomicU16::new(0),
            io_lock: IrqSpinLock::with_name((), "virtio-scsi-io"),
            block_size: VIRTIO_SCSI_SECTOR_SIZE_DEFAULT,
            block_count: 0,
            initialized: AtomicBool::new(false),
        }
    }

    unsafe fn read_cap_byte(addr: PciAddress, offset: u8) -> u8 {
        let aligned_offset = offset & 0xFC;
        let shift = (offset & 0x3) * 8;
        let val = pci::read_config_32(addr, aligned_offset);
        ((val >> shift) & 0xFF) as u8
    }

    unsafe fn read_cap_dword(addr: PciAddress, offset: u8) -> u32 {
        pci::read_config_32(addr, offset & 0xFC)
    }

    fn find_capability(addr: PciAddress, cap_id: u8) -> Option<VirtioCap> {
        if (pci::read_header(addr).status & (1 << 4)) == 0 {
            return None;
        }
        let caps_ptr = (unsafe { pci::read_config_32(addr, 0x34) } as u8) & !0x3;
        if caps_ptr == 0 {
            return None;
        }

        let mut offset = caps_ptr;
        let mut count = 0u8;
        while offset != 0 && count < 48 {
            let cap_vndr = unsafe { Self::read_cap_byte(addr, offset) };
            let cap_next = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(1)) } & !0x3;
            let cap_len = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(2)) };

            if cap_vndr == 0x09 {
                let cfg_type = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(3)) };
                if cfg_type == cap_id {
                    let bar = unsafe { Self::read_cap_byte(addr, offset.wrapping_add(4)) };
                    let cap_offset = unsafe { Self::read_cap_dword(addr, offset.wrapping_add(8)) };
                    let length = unsafe { Self::read_cap_dword(addr, offset.wrapping_add(12)) };
                    let notify_off_multiplier =
                        if cap_id == VIRTIO_PCI_CAP_NOTIFY_CFG && cap_len >= 20 {
                            unsafe { Self::read_cap_dword(addr, offset.wrapping_add(16)) }
                        } else {
                            0
                        };
                    return Some(VirtioCap {
                        bar,
                        offset: cap_offset,
                        length,
                        notify_off_multiplier,
                    });
                }
            }

            if cap_next == offset {
                break;
            }
            offset = cap_next;
            count += 1;
        }
        None
    }

    fn read_bar_phys(addr: PciAddress, bar_index: u8) -> Result<u64, BlockError> {
        let bar_offset: u8 = (0x10 + (bar_index as u16 * 4)) as u8;
        let bar_lo = unsafe { pci::read_config_32(addr, bar_offset) };
        if (bar_lo & 0x1) != 0 {
            return Err(BlockError::Unsupported);
        }
        if bar_lo == 0 || bar_lo == 0xFFFFFFF0 {
            return Err(BlockError::IoError);
        }
        let mem_type = (bar_lo >> 1) & 0x3;
        if mem_type == 1 || mem_type == 3 {
            return Err(BlockError::Unsupported);
        }

        let mut mmio_phys = (bar_lo & 0xFFFF_FFF0) as u64;
        if mem_type == 2 {
            if bar_index >= 5 {
                return Err(BlockError::Unsupported);
            }
            let bar1 = unsafe { pci::read_config_32(addr, bar_offset + 4) };
            mmio_phys |= (bar1 as u64) << 32;
        }
        if mmio_phys == 0 {
            return Err(BlockError::IoError);
        }
        Ok(mmio_phys)
    }

    fn map_capability(
        addr: PciAddress,
        cap: &VirtioCap,
        min_len: usize,
    ) -> Result<*mut u8, BlockError> {
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
            return Err(BlockError::IoError);
        }
        Ok(mmio_virt)
    }

    fn init_modern(&mut self, addr: PciAddress, _header: &PciHeader) -> Result<(), BlockError> {
        let cmd = unsafe { pci::read_config_32(addr, 0x04) };
        let new_cmd = (cmd & 0xFFFF_0000) | ((cmd as u16 | 0x06) as u32);
        if new_cmd != cmd {
            unsafe { pci::write_config_32(addr, 0x04, new_cmd) };
        }

        let common_cap = Self::find_capability(addr, VIRTIO_PCI_CAP_COMMON_CFG)
            .ok_or(BlockError::Unsupported)?;
        let notify_cap = Self::find_capability(addr, VIRTIO_PCI_CAP_NOTIFY_CFG)
            .ok_or(BlockError::Unsupported)?;
        let device_cap = Self::find_capability(addr, VIRTIO_PCI_CAP_DEVICE_CFG)
            .ok_or(BlockError::Unsupported)?;

        self.common_cfg = Self::map_capability(
            addr,
            &common_cap,
            core::mem::size_of::<VirtioPciCommonCfg>(),
        )? as *mut VirtioPciCommonCfg;
        self.config =
            Self::map_capability(addr, &device_cap, core::mem::size_of::<VirtioScsiConfig>())?
                as *const VirtioScsiConfig;
        self.notify_base = Self::map_capability(addr, &notify_cap, 2)?;
        self.notify_len = core::cmp::max(notify_cap.length as usize, 2);
        self.notify_off_multiplier = notify_cap.notify_off_multiplier;
        if self.notify_off_multiplier == 0 {
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

        let features_hi = self.read_device_features(1);
        if (features_hi & VIRTIO_F_VERSION_1) == 0 {
            return Err(BlockError::Unsupported);
        }

        self.write_driver_features(0, 0);
        self.write_driver_features(1, VIRTIO_F_VERSION_1);
        self.set_status(
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        );
        if (self.get_status() & VIRTIO_STATUS_FEATURES_OK) == 0 {
            return Err(BlockError::Unsupported);
        }

        self.setup_request_queue()?;
        self.probe_lun0()?;

        self.set_status(
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );
        self.initialized.store(true, Ordering::SeqCst);
        ok!(
            "[virtio-scsi] blocks={} block_size={}",
            self.block_count,
            self.block_size
        );
        Ok(())
    }

    fn setup_request_queue(&mut self) -> Result<(), BlockError> {
        unsafe { write_volatile(&mut (*self.common_cfg).queue_select, VIRTIO_SCSI_REQUESTQ) };
        self.queue_size = unsafe { read_volatile(&(*self.common_cfg).queue_size) as u32 };
        if self.queue_size == 0 {
            return Err(BlockError::IoError);
        }
        self.queue_size = self.queue_size.min(128);
        unsafe { write_volatile(&mut (*self.common_cfg).queue_size, self.queue_size as u16) };

        let n = self.queue_size as usize;
        let desc_size = n * core::mem::size_of::<VirtqDesc>();
        let avail_size = 4 + n * 2;
        let used_size =
            page_align_up((4 + n * core::mem::size_of::<VirtqUsedElem>()) as u64) as usize;
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

        self.req_mem = DmaBuffer::new(core::mem::size_of::<VirtioScsiReq>()).into();
        self.resp_mem = DmaBuffer::new(core::mem::size_of::<VirtioScsiResp>()).into();
        self.data_mem = DmaBuffer::new(DATA_BUFFER_SIZE).into();

        let notify_off = unsafe { read_volatile(&(*self.common_cfg).queue_notify_off) } as usize;
        let notify_offset = notify_off
            .checked_mul(self.notify_off_multiplier as usize)
            .ok_or(BlockError::IoError)?;
        if notify_offset + core::mem::size_of::<u16>() > self.notify_len {
            return Err(BlockError::IoError);
        }
        self.queue_notify = unsafe { self.notify_base.add(notify_offset) as *mut u16 };
        crate::mm::vmalloc::set_vmalloc_watch_page(self.queue_notify as u64);
        if !crate::mm::vmalloc::ensure_vmalloc_page_mapped_in_current(self.queue_notify as u64) {
            error!(
                "[virtio-scsi] queue notify mapping missing during setup: queue_notify={:#x} off={:#x} len={:#x}",
                self.queue_notify as u64, notify_offset, self.notify_len
            );
            return Err(BlockError::IoError);
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

    fn submit_cmd(
        &self,
        cdb: &[u8],
        read_len: usize,
        out: Option<&mut [u8]>,
    ) -> Result<usize, BlockError> {
        let _io_guard = self.io_lock.lock();
        if !self.req_ready() {
            return Err(BlockError::NotReady);
        }
        if cdb.len() > MAX_CDB_SIZE || read_len > DATA_BUFFER_SIZE {
            return Err(BlockError::Unsupported);
        }

        let req_mem = self.req_mem.as_ref().ok_or(BlockError::NotReady)?;
        let resp_mem = self.resp_mem.as_ref().ok_or(BlockError::NotReady)?;
        let data_mem = self.data_mem.as_ref().ok_or(BlockError::NotReady)?;

        unsafe {
            core::ptr::write_bytes(req_mem.as_ptr(), 0, core::mem::size_of::<VirtioScsiReq>());
            core::ptr::write_bytes(resp_mem.as_ptr(), 0, core::mem::size_of::<VirtioScsiResp>());
        }
        let req = req_mem.as_ptr() as *mut VirtioScsiReq;
        unsafe {
            core::ptr::copy_nonoverlapping(
                LUN0.as_ptr(),
                core::ptr::addr_of_mut!((*req).lun) as *mut u8,
                LUN0.len(),
            );
            (*req).id = 0;
            (*req).task_attr = 0;
            (*req).prio = 0;
            (*req).crn = 0;
            core::ptr::copy_nonoverlapping(
                cdb.as_ptr(),
                core::ptr::addr_of_mut!((*req).cdb) as *mut u8,
                cdb.len(),
            );
        }

        let desc = self.desc;
        unsafe {
            (*desc.add(0)).addr = req_mem.phys_addr();
            (*desc.add(0)).len = core::mem::size_of::<VirtioScsiReq>() as u32;
            (*desc.add(0)).flags = VIRTQ_DESC_F_NEXT;
            (*desc.add(0)).next = 1;

            (*desc.add(1)).addr = data_mem.phys_addr();
            (*desc.add(1)).len = read_len as u32;
            (*desc.add(1)).flags = VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE;
            (*desc.add(1)).next = 2;

            (*desc.add(2)).addr = resp_mem.phys_addr();
            (*desc.add(2)).len = core::mem::size_of::<VirtioScsiResp>() as u32;
            (*desc.add(2)).flags = VIRTQ_DESC_F_WRITE;
            (*desc.add(2)).next = 0;
        }

        let idx = self.avail_idx.fetch_add(1, Ordering::SeqCst);
        let ring_idx = (idx % self.queue_size as u16) as usize;
        unsafe {
            let ring = (self.avail as *mut u8).add(4) as *mut u16;
            *ring.add(ring_idx) = 0;
            core::sync::atomic::fence(Ordering::SeqCst);
            write_volatile(&mut (*self.avail).idx, idx.wrapping_add(1));
        }

        if !crate::mm::vmalloc::ensure_vmalloc_page_mapped_in_current(self.queue_notify as u64) {
            return Err(BlockError::IoError);
        }
        unsafe { write_volatile(self.queue_notify, VIRTIO_SCSI_REQUESTQ) };

        let target_used = idx.wrapping_add(1);
        for _ in 0..1_000_000 {
            let used_idx = unsafe { read_volatile(&(*self.used).idx) };
            if used_idx == target_used {
                let resp = unsafe { &*(resp_mem.as_ptr() as *const VirtioScsiResp) };
                if resp.response != VIRTIO_SCSI_S_OK {
                    return Err(BlockError::IoError);
                }
                if resp.status != SCSI_STATUS_GOOD {
                    return Err(BlockError::IoError);
                }
                let transferred = read_len.saturating_sub(resp.residual as usize);
                if let Some(buf) = out {
                    let copy_len = transferred.min(buf.len());
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data_mem.as_ptr(),
                            buf.as_mut_ptr(),
                            copy_len,
                        );
                    }
                    return Ok(copy_len);
                }
                return Ok(transferred);
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn req_ready(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
            || (!self.common_cfg.is_null() && !self.queue_notify.is_null())
    }

    fn probe_lun0(&mut self) -> Result<(), BlockError> {
        let mut inquiry = [0u8; INQUIRY_DATA_LEN];
        let inquiry_cdb = [SCSI_CMD_INQUIRY, 0, 0, 0, INQUIRY_DATA_LEN as u8, 0];
        let got = self.submit_cmd(&inquiry_cdb, INQUIRY_DATA_LEN, Some(&mut inquiry))?;
        if got < 5 || (inquiry[0] & 0x1f) != 0 {
            return Err(BlockError::Unsupported);
        }

        let mut cap = [0u8; READ_CAPACITY10_DATA_LEN];
        let cap_cdb = [SCSI_CMD_READ_CAPACITY10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let got = self.submit_cmd(&cap_cdb, READ_CAPACITY10_DATA_LEN, Some(&mut cap))?;
        if got < READ_CAPACITY10_DATA_LEN {
            return Err(BlockError::IoError);
        }
        let last_lba = u32::from_be_bytes([cap[0], cap[1], cap[2], cap[3]]);
        let block_size = u32::from_be_bytes([cap[4], cap[5], cap[6], cap[7]]);
        if last_lba == u32::MAX || block_size == 0 || block_size as usize > DATA_BUFFER_SIZE {
            return Err(BlockError::Unsupported);
        }
        self.block_size = block_size;
        self.block_count = last_lba as u64 + 1;
        info!(
            "[virtio-scsi] direct-access lun0 blocks={} block_size={}",
            self.block_count, self.block_size
        );
        Ok(())
    }

    fn read_block_inner(&self, lba: u64, data: &mut [u8]) -> Result<(), BlockError> {
        let transfer_len = self.block_size as usize;
        if data.len() < transfer_len {
            return Err(BlockError::InvalidBufferSize);
        }
        let lba32 = u32::try_from(lba).map_err(|_| BlockError::InvalidAddress)?;
        let blocks16 = 1u16.to_be_bytes();
        let lba_bytes = lba32.to_be_bytes();
        let cdb = [
            SCSI_CMD_READ10,
            0,
            lba_bytes[0],
            lba_bytes[1],
            lba_bytes[2],
            lba_bytes[3],
            0,
            blocks16[0],
            blocks16[1],
            0,
        ];
        let read = self.submit_cmd(&cdb, transfer_len, Some(&mut data[..transfer_len]))?;
        if read < transfer_len {
            return Err(BlockError::IoError);
        }
        Ok(())
    }
}

impl BlockDevice for VirtioScsiDevice {
    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn name(&self) -> &str {
        "virtio-scsi"
    }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(BlockError::NotReady);
        }
        if lba >= self.block_count {
            return Err(BlockError::InvalidAddress);
        }
        self.read_block_inner(lba, buf)
    }

    fn write_block(&self, _lba: u64, _buf: &[u8]) -> Result<(), BlockError> {
        Err(BlockError::WriteProtected)
    }

    fn flush(&self) -> Result<(), BlockError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(BlockError::NotReady);
        }
        Ok(())
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

static VIRTIO_SCSI: IrqSpinLock<VirtioScsiDevice> = IrqSpinLock::new(VirtioScsiDevice::new());

struct VirtioScsiDriver;

impl PciDriver for VirtioScsiDriver {
    fn name(&self) -> &'static str {
        "virtio-scsi"
    }

    fn supported_ids(&self) -> &[PciDeviceId] {
        static IDS: [PciDeviceId; 2] = [
            PciDeviceId::vendor_device(VIRTIO_PCI_VENDOR_ID, VIRTIO_PCI_DEVICE_TRANSITIONAL_SCSI),
            PciDeviceId::vendor_device(VIRTIO_PCI_VENDOR_ID, VIRTIO_PCI_DEVICE_MODERN_SCSI),
        ];
        &IDS
    }

    fn probe(&self, addr: PciAddress, header: &PciHeader) -> ProbeResult {
        let mut dev = VIRTIO_SCSI.lock();
        match dev.init_modern(addr, header) {
            Ok(()) => ProbeResult::Claimed,
            Err(BlockError::Unsupported) => {
                warn!("[virtio-scsi] unsupported transport/features");
                ProbeResult::Unsupported
            }
            Err(err) => {
                error!("[virtio-scsi] init failed ({:?})", err);
                ProbeResult::Error("init failed")
            }
        }
    }
}

pub fn register() {
    pci::register_driver(Box::new(VirtioScsiDriver));
}

pub fn get_device() -> Option<&'static VirtioScsiDevice> {
    let dev = VIRTIO_SCSI.lock();
    if dev.initialized.load(Ordering::SeqCst) {
        Some(unsafe { &*(&*dev as *const VirtioScsiDevice) })
    } else {
        None
    }
}

pub fn init() {
    register();
}
