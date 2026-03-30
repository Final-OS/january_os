//! Block device tests

use crate::drivers::block::{self, BlockDevice};
use crate::sync::Mutex;
use crate::{error, kprintln, ok, warn};

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    kprintln!("=== Block Device Tests ===");
    block_step("start block test suite");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/block] filter={:?}", filter);
    }

    match filter {
        None | Some("all") => {
            block_step("run case=virtio");
            test_virtio_blk();
            block_step("run case=virtio-scsi");
            test_virtio_scsi();
            block_step("run case=partition");
            test_partition();
        }
        Some("virtio") => {
            block_step("run case=virtio");
            test_virtio_blk();
        }
        Some("partition") => {
            block_step("run case=partition");
            test_partition();
        }
        Some("virtio-scsi") => {
            block_step("run case=virtio-scsi");
            test_virtio_scsi();
        }
        Some(name) => {
            error!("Unknown block test: {}", name);
            kprintln!("Available block tests: virtio, virtio-scsi, partition");
        }
    }

    block_step("block test suite done");
    kprintln!();
}

fn test_virtio_blk() {
    block_step("check virtio-blk device");

    // Driver is registered during boot, devices are probed by PCI subsystem
    if let Some(dev) = block::virtio_blk::get_device() {
        pass("virtio_device_found");

        block_step("verify device properties");

        let block_size = dev.block_size();
        let block_count = dev.block_count();

        kprintln!(
            "[test/block] block_size={} block_count={}",
            block_size,
            block_count
        );

        if block_size == 512 {
            pass("virtio_block_size");
        } else {
            fail("virtio_block_size", "expected 512");
        }

        if block_count > 0 {
            pass("virtio_capacity");

            block_step("read first block");
            let mut buf = [0u8; 512];
            match dev.read_block(0, &mut buf) {
                Ok(()) => {
                    pass("virtio_read");
                    kprintln!("[test/block] first 16 bytes: {:02x?}", &buf[..16]);
                }
                Err(_) => {
                    fail("virtio_read", "read failed");
                }
            }

            if !dev.is_read_only() {
                block_step("write test (not read-only)");
                let test_data = [0xDEu8; 512];
                match dev.write_block(0, &test_data) {
                    Ok(()) => {
                        pass("virtio_write");
                    }
                    Err(_) => {
                        fail("virtio_write", "write failed");
                    }
                }
            } else {
                block_step("device is read-only, skip write test");
                pass("virtio_readonly_check");
            }
        } else {
            fail("virtio_capacity", "zero capacity");
        }
    } else {
        pass("virtio_init_skipped");
        warn!("virtio-blk device not available (this is OK if no virtio disk attached)");
    }
}

fn test_virtio_scsi() {
    block_step("check virtio-scsi device");

    if let Some(dev) = block::virtio_scsi::get_device() {
        pass("virtio_scsi_device_found");

        let block_size = dev.block_size();
        let block_count = dev.block_count();
        kprintln!(
            "[test/block] virtio-scsi block_size={} block_count={}",
            block_size,
            block_count
        );

        if block_size >= 512 {
            pass("virtio_scsi_block_size");
        } else {
            fail("virtio_scsi_block_size", "expected >= 512");
        }

        if block_count > 0 {
            pass("virtio_scsi_capacity");
            let mut buf = [0u8; 512];
            match dev.read_block(0, &mut buf) {
                Ok(()) => {
                    pass("virtio_scsi_read");
                    kprintln!(
                        "[test/block] virtio-scsi first 16 bytes: {:02x?}",
                        &buf[..16]
                    );
                }
                Err(_) => fail("virtio_scsi_read", "read failed"),
            }
            if dev.is_read_only() {
                pass("virtio_scsi_readonly_check");
            } else {
                fail(
                    "virtio_scsi_readonly_check",
                    "expected readonly minimal chain",
                );
            }
        } else {
            fail("virtio_scsi_capacity", "zero capacity");
        }
    } else {
        pass("virtio_scsi_init_skipped");
        warn!("virtio-scsi device not available (this is OK if no virtio-scsi disk attached)");
    }
}

const MOCK_BLOCK_SIZE: usize = 512;

struct MemBlockDevice {
    name: &'static str,
    block_count: u64,
    data: Mutex<alloc::vec::Vec<u8>>,
}

impl MemBlockDevice {
    fn new(name: &'static str, block_count: u64) -> Result<Self, &'static str> {
        let total_bytes = (block_count as usize)
            .checked_mul(MOCK_BLOCK_SIZE)
            .ok_or("mock device size overflow")?;
        Ok(Self {
            name,
            block_count,
            data: Mutex::new(vec![0u8; total_bytes]),
        })
    }

    fn raw_offset(&self, lba: u64) -> Result<usize, &'static str> {
        if lba >= self.block_count {
            return Err("mock lba out of range");
        }
        (lba as usize)
            .checked_mul(MOCK_BLOCK_SIZE)
            .ok_or("mock offset overflow")
    }

    fn read_raw_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        if buf.len() != MOCK_BLOCK_SIZE {
            return Err("mock read invalid size");
        }
        let off = self.raw_offset(lba)?;
        let data = self.data.lock();
        buf.copy_from_slice(&data[off..off + MOCK_BLOCK_SIZE]);
        Ok(())
    }

    fn write_raw_block(&self, lba: u64, buf: &[u8]) -> Result<(), &'static str> {
        if buf.len() != MOCK_BLOCK_SIZE {
            return Err("mock write invalid size");
        }
        let off = self.raw_offset(lba)?;
        let mut data = self.data.lock();
        data[off..off + MOCK_BLOCK_SIZE].copy_from_slice(buf);
        Ok(())
    }
}

impl BlockDevice for MemBlockDevice {
    fn block_size(&self) -> u32 {
        MOCK_BLOCK_SIZE as u32
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn name(&self) -> &str {
        self.name
    }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), block::BlockError> {
        self.read_raw_block(lba, buf)
            .map_err(|_| block::BlockError::IoError)
    }

    fn write_block(&self, lba: u64, buf: &[u8]) -> Result<(), block::BlockError> {
        self.write_raw_block(lba, buf)
            .map_err(|_| block::BlockError::IoError)
    }
}

fn test_partition() {
    match test_partition_mbr_case() {
        Ok(()) => pass("partition_mbr"),
        Err(msg) => fail("partition_mbr", msg.as_str()),
    }

    match test_partition_gpt_case() {
        Ok(()) => pass("partition_gpt"),
        Err(msg) => fail("partition_gpt", msg.as_str()),
    }
}

fn test_partition_mbr_case() -> Result<(), String> {
    let dev = Arc::new(MemBlockDevice::new("mock-mbr", 512).map_err(String::from)?);

    let mut mbr = [0u8; MOCK_BLOCK_SIZE];
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    let entry = 446usize;
    mbr[entry + 4] = 0x83;
    mbr[entry + 8..entry + 12].copy_from_slice(&(64u32).to_le_bytes());
    mbr[entry + 12..entry + 16].copy_from_slice(&(128u32).to_le_bytes());
    dev.write_raw_block(0, &mbr).map_err(String::from)?;

    let parsed = block::discover_partitions(dev.clone() as Arc<dyn BlockDevice>)
        .map_err(|e| format!("discover MBR failed: {}", e))?;
    if parsed.table_kind() != block::PartitionTableKind::Mbr {
        return Err(String::from("expected MBR table kind"));
    }
    if parsed.partitions().len() != 1 {
        return Err(String::from("expected exactly 1 MBR partition"));
    }
    let part = parsed.partitions()[0];
    if part.start_lba != 64 || part.block_count != 128 {
        return Err(String::from("unexpected MBR partition geometry"));
    }

    let part_dev = parsed
        .open_partition(1)
        .map_err(|e| format!("open partition failed: {}", e))?;
    let pattern = [0xA5u8; MOCK_BLOCK_SIZE];
    part_dev
        .write_block(0, &pattern)
        .map_err(|_| String::from("partition write failed"))?;

    let mut parent = [0u8; MOCK_BLOCK_SIZE];
    dev.read_raw_block(64, &mut parent).map_err(String::from)?;
    if parent != pattern {
        return Err(String::from(
            "partition write did not translate to parent LBA",
        ));
    }

    Ok(())
}

fn test_partition_gpt_case() -> Result<(), String> {
    let block_count = 512u64;
    let dev = Arc::new(MemBlockDevice::new("mock-gpt", block_count).map_err(String::from)?);

    let entry_count = 4u32;
    let entry_size = 128u32;
    let mut entries = [0u8; MOCK_BLOCK_SIZE];
    let linux_fs_guid_raw: [u8; 16] = [
        0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D,
        0xE4,
    ];
    entries[0..16].copy_from_slice(&linux_fs_guid_raw);
    entries[16..32].copy_from_slice(&[
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70,
        0x80,
    ]);
    entries[32..40].copy_from_slice(&(64u64).to_le_bytes());
    entries[40..48].copy_from_slice(&(127u64).to_le_bytes());
    let entries_crc = block::gpt::crc32(&entries);

    let mut hdr = [0u8; MOCK_BLOCK_SIZE];
    hdr[0..8].copy_from_slice(b"EFI PART");
    hdr[8..12].copy_from_slice(&(0x0001_0000u32).to_le_bytes());
    hdr[12..16].copy_from_slice(&(92u32).to_le_bytes());
    hdr[24..32].copy_from_slice(&(1u64).to_le_bytes());
    hdr[32..40].copy_from_slice(&(block_count - 1).to_le_bytes());
    hdr[40..48].copy_from_slice(&(34u64).to_le_bytes());
    hdr[48..56].copy_from_slice(&(block_count - 34).to_le_bytes());
    hdr[56..72].copy_from_slice(&[
        0xAA, 0xBB, 0xCC, 0xDD, 0x01, 0x02, 0x03, 0x04, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
        0x17,
    ]);
    hdr[72..80].copy_from_slice(&(2u64).to_le_bytes());
    hdr[80..84].copy_from_slice(&entry_count.to_le_bytes());
    hdr[84..88].copy_from_slice(&entry_size.to_le_bytes());
    hdr[88..92].copy_from_slice(&entries_crc.to_le_bytes());

    let mut crc_area = [0u8; 92];
    crc_area.copy_from_slice(&hdr[0..92]);
    crc_area[16..20].fill(0);
    let hdr_crc = block::gpt::crc32(&crc_area);
    hdr[16..20].copy_from_slice(&hdr_crc.to_le_bytes());

    dev.write_raw_block(1, &hdr).map_err(String::from)?;
    dev.write_raw_block(2, &entries).map_err(String::from)?;

    let parsed = block::discover_partitions(dev.clone() as Arc<dyn BlockDevice>)
        .map_err(|e| format!("discover GPT failed: {}", e))?;
    if parsed.table_kind() != block::PartitionTableKind::Gpt {
        return Err(String::from("expected GPT table kind"));
    }
    if parsed.partitions().len() != 1 {
        return Err(String::from("expected exactly 1 GPT partition"));
    }
    let part = parsed.partitions()[0];
    if part.start_lba != 64 || part.block_count != (127 - 64 + 1) {
        return Err(String::from("unexpected GPT partition geometry"));
    }

    let part_dev = parsed
        .open_partition(1)
        .map_err(|e| format!("open GPT partition failed: {}", e))?;
    let pattern = [0x5Au8; MOCK_BLOCK_SIZE];
    part_dev
        .write_block(0, &pattern)
        .map_err(|_| String::from("GPT partition write failed"))?;

    let mut parent = [0u8; MOCK_BLOCK_SIZE];
    dev.read_raw_block(64, &mut parent).map_err(String::from)?;
    if parent != pattern {
        return Err(String::from(
            "GPT partition write did not translate to parent LBA",
        ));
    }

    Ok(())
}

pub(super) fn pass(name: &str) {
    ok!("block/{}", name);
}

pub(super) fn fail(name: &str, msg: &str) {
    error!("block/{}: {}", name, msg);
}

fn block_step(msg: &str) {
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/block][step] {}", msg);
    }
}
