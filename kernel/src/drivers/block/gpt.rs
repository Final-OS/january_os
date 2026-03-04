//! GPT partition parser with CRC32 validation.

use super::BlockDevice;
use super::partition::{Partition, PartitionError, PartitionType};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_HEADER_MIN_SIZE: usize = 92;
const GPT_ENTRY_MIN_SIZE: usize = 128;
const GPT_MAX_ENTRIES_BYTES: usize = 16 * 1024 * 1024;

#[inline]
fn read_le_u32(bytes: &[u8]) -> u32 {
    (bytes[0] as u32)
        | ((bytes[1] as u32) << 8)
        | ((bytes[2] as u32) << 16)
        | ((bytes[3] as u32) << 24)
}

#[inline]
fn read_le_u64(bytes: &[u8]) -> u64 {
    (bytes[0] as u64)
        | ((bytes[1] as u64) << 8)
        | ((bytes[2] as u64) << 16)
        | ((bytes[3] as u64) << 24)
        | ((bytes[4] as u64) << 32)
        | ((bytes[5] as u64) << 40)
        | ((bytes[6] as u64) << 48)
        | ((bytes[7] as u64) << 56)
}

#[inline]
fn is_zero_guid(raw: &[u8; 16]) -> bool {
    raw.iter().all(|b| *b == 0)
}

/// Convert GPT on-disk GUID bytes into canonical byte order.
#[inline]
fn normalize_gpt_guid(raw: &[u8; 16]) -> [u8; 16] {
    [
        raw[3], raw[2], raw[1], raw[0], raw[5], raw[4], raw[7], raw[6], raw[8], raw[9], raw[10], raw[11], raw[12],
        raw[13], raw[14], raw[15],
    ]
}

fn read_blocks(device: &Arc<dyn BlockDevice>, start_lba: u64, block_count: usize) -> Result<Vec<u8>, PartitionError> {
    let block_size = device.block_size() as usize;
    let mut out = vec![0u8; block_size.saturating_mul(block_count)];
    let mut block = vec![0u8; block_size];
    for i in 0..block_count {
        let lba = start_lba
            .checked_add(i as u64)
            .ok_or(PartitionError::OutOfRange)?;
        device.read_block(lba, &mut block)?;
        let dst = i * block_size;
        out[dst..dst + block_size].copy_from_slice(&block);
    }
    Ok(out)
}

pub fn parse_gpt_partitions(device: Arc<dyn BlockDevice>) -> Result<Option<Vec<Partition>>, PartitionError> {
    let block_size = device.block_size() as usize;
    if block_size < GPT_HEADER_MIN_SIZE {
        return Err(PartitionError::InvalidGpt("block size too small"));
    }
    if device.block_count() < 2 {
        return Err(PartitionError::InvalidGpt("device too small"));
    }

    let mut header_block = vec![0u8; block_size];
    device.read_block(1, &mut header_block)?;

    if &header_block[0..8] != GPT_SIGNATURE {
        return Ok(None);
    }

    let header_size = read_le_u32(&header_block[12..16]) as usize;
    if !(GPT_HEADER_MIN_SIZE..=block_size).contains(&header_size) {
        return Err(PartitionError::InvalidGpt("invalid header size"));
    }

    let stored_header_crc = read_le_u32(&header_block[16..20]);
    let mut crc_buf = header_block[..header_size].to_vec();
    crc_buf[16] = 0;
    crc_buf[17] = 0;
    crc_buf[18] = 0;
    crc_buf[19] = 0;
    if crc32(&crc_buf) != stored_header_crc {
        return Err(PartitionError::InvalidGpt("header CRC mismatch"));
    }

    let current_lba = read_le_u64(&header_block[24..32]);
    if current_lba != 1 {
        return Err(PartitionError::InvalidGpt("current_lba != 1"));
    }

    let first_usable_lba = read_le_u64(&header_block[40..48]);
    let last_usable_lba = read_le_u64(&header_block[48..56]);
    if first_usable_lba > last_usable_lba || last_usable_lba >= device.block_count() {
        return Err(PartitionError::OutOfRange);
    }

    let entries_lba = read_le_u64(&header_block[72..80]);
    let entry_count = read_le_u32(&header_block[80..84]) as usize;
    let entry_size = read_le_u32(&header_block[84..88]) as usize;
    let stored_entries_crc = read_le_u32(&header_block[88..92]);
    if entry_count == 0 {
        return Err(PartitionError::NoPartitions);
    }
    if entry_size < GPT_ENTRY_MIN_SIZE {
        return Err(PartitionError::InvalidGpt("entry size < 128"));
    }

    let entries_bytes = entry_count
        .checked_mul(entry_size)
        .ok_or(PartitionError::OutOfRange)?;
    if entries_bytes > GPT_MAX_ENTRIES_BYTES {
        return Err(PartitionError::Unsupported("GPT entry table is too large"));
    }

    let blocks_needed = entries_bytes
        .checked_add(block_size - 1)
        .ok_or(PartitionError::OutOfRange)?
        / block_size;

    let entries_end = entries_lba
        .checked_add(blocks_needed as u64)
        .ok_or(PartitionError::OutOfRange)?;
    if entries_lba == 0 || entries_end > device.block_count() {
        return Err(PartitionError::OutOfRange);
    }

    let mut entries = read_blocks(&device, entries_lba, blocks_needed)?;
    entries.truncate(entries_bytes);
    if crc32(&entries) != stored_entries_crc {
        return Err(PartitionError::InvalidGpt("entry table CRC mismatch"));
    }

    let mut partitions = Vec::new();
    for i in 0..entry_count {
        let base = i * entry_size;
        let mut type_guid_raw = [0u8; 16];
        type_guid_raw.copy_from_slice(&entries[base..base + 16]);
        if is_zero_guid(&type_guid_raw) {
            continue;
        }

        let first_lba = read_le_u64(&entries[base + 32..base + 40]);
        let last_lba = read_le_u64(&entries[base + 40..base + 48]);
        if first_lba == 0 || first_lba > last_lba {
            return Err(PartitionError::InvalidGpt("invalid partition LBA range"));
        }
        if first_lba < first_usable_lba || last_lba > last_usable_lba {
            return Err(PartitionError::OutOfRange);
        }

        let block_count = last_lba
            .checked_sub(first_lba)
            .and_then(|v| v.checked_add(1))
            .ok_or(PartitionError::OutOfRange)?;

        partitions.push(Partition {
            index: (i + 1) as u32,
            start_lba: first_lba,
            block_count,
            part_type: PartitionType::Gpt(normalize_gpt_guid(&type_guid_raw)),
        });
    }

    if partitions.is_empty() {
        return Err(PartitionError::NoPartitions);
    }
    Ok(Some(partitions))
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let lsb = crc & 1;
            crc >>= 1;
            if lsb != 0 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}
