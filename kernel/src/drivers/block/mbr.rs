//! MBR partition parser.

use super::BlockDevice;
use super::partition::{Partition, PartitionError, PartitionType};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

const MBR_SIGNATURE_OFF: usize = 510;
const MBR_SIGNATURE0: u8 = 0x55;
const MBR_SIGNATURE1: u8 = 0xAA;
const MBR_PARTITION_TABLE_OFF: usize = 446;
const MBR_PARTITION_COUNT: usize = 4;
const MBR_ENTRY_SIZE: usize = 16;
const MBR_MIN_LEN: usize = 512;

const MBR_TYPE_EXTENDED_CHS: u8 = 0x05;
const MBR_TYPE_EXTENDED_LBA: u8 = 0x0F;
const MBR_TYPE_EXTENDED_LINUX: u8 = 0x85;

#[inline]
fn read_le_u32(bytes: &[u8]) -> u32 {
    (bytes[0] as u32)
        | ((bytes[1] as u32) << 8)
        | ((bytes[2] as u32) << 16)
        | ((bytes[3] as u32) << 24)
}

#[inline]
fn is_extended_type(ty: u8) -> bool {
    ty == MBR_TYPE_EXTENDED_CHS || ty == MBR_TYPE_EXTENDED_LBA || ty == MBR_TYPE_EXTENDED_LINUX
}

pub fn parse_mbr_partitions(device: Arc<dyn BlockDevice>) -> Result<Option<Vec<Partition>>, PartitionError> {
    let block_size = device.block_size() as usize;
    if block_size < MBR_MIN_LEN {
        return Err(PartitionError::InvalidMbr("block size < 512"));
    }

    let mut sector0 = vec![0u8; block_size];
    device.read_block(0, &mut sector0)?;

    if sector0[MBR_SIGNATURE_OFF] != MBR_SIGNATURE0 || sector0[MBR_SIGNATURE_OFF + 1] != MBR_SIGNATURE1 {
        return Ok(None);
    }

    let total_blocks = device.block_count();
    let mut partitions = Vec::new();
    let mut saw_extended = false;

    for slot in 0..MBR_PARTITION_COUNT {
        let base = MBR_PARTITION_TABLE_OFF + slot * MBR_ENTRY_SIZE;
        let ptype = sector0[base + 4];
        let first_lba = read_le_u32(&sector0[base + 8..base + 12]) as u64;
        let sector_count = read_le_u32(&sector0[base + 12..base + 16]) as u64;

        if ptype == 0 || sector_count == 0 {
            continue;
        }
        if is_extended_type(ptype) {
            saw_extended = true;
            continue;
        }
        if first_lba == 0 {
            return Err(PartitionError::InvalidMbr("partition starts at LBA 0"));
        }

        let end = first_lba
            .checked_add(sector_count)
            .ok_or(PartitionError::OutOfRange)?;
        if end > total_blocks {
            return Err(PartitionError::OutOfRange);
        }

        partitions.push(Partition {
            index: (slot + 1) as u32,
            start_lba: first_lba,
            block_count: sector_count,
            part_type: PartitionType::Mbr(ptype),
        });
    }

    if partitions.is_empty() {
        if saw_extended {
            return Err(PartitionError::Unsupported("extended MBR partitions"));
        }
        return Err(PartitionError::NoPartitions);
    }

    Ok(Some(partitions))
}
