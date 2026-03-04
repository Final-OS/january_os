//! Partition table abstraction and partition-backed block devices.

use super::{BlockDevice, BlockError, gpt, mbr};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTableKind {
    Mbr,
    Gpt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionType {
    Mbr(u8),
    Gpt([u8; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Partition {
    pub index: u32,
    pub start_lba: u64,
    pub block_count: u64,
    pub part_type: PartitionType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionError {
    NoPartitionTable,
    NoPartitions,
    InvalidMbr(&'static str),
    InvalidGpt(&'static str),
    Unsupported(&'static str),
    OutOfRange,
    Io(BlockError),
}

impl fmt::Display for PartitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartitionError::NoPartitionTable => write!(f, "No partition table found"),
            PartitionError::NoPartitions => write!(f, "Partition table has no usable entries"),
            PartitionError::InvalidMbr(msg) => write!(f, "Invalid MBR: {}", msg),
            PartitionError::InvalidGpt(msg) => write!(f, "Invalid GPT: {}", msg),
            PartitionError::Unsupported(msg) => write!(f, "Unsupported partition feature: {}", msg),
            PartitionError::OutOfRange => write!(f, "Partition range is out of device bounds"),
            PartitionError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl From<BlockError> for PartitionError {
    fn from(value: BlockError) -> Self {
        PartitionError::Io(value)
    }
}

/// Block device wrapper for a single partition.
pub struct PartitionBlockDevice {
    device: Arc<dyn BlockDevice>,
    partition: Partition,
    part_name: String,
}

impl PartitionBlockDevice {
    pub fn new(device: Arc<dyn BlockDevice>, partition: Partition) -> Self {
        Self {
            part_name: format!("{}p{}", device.name(), partition.index),
            device,
            partition,
        }
    }

    #[inline]
    fn translate_lba(&self, lba: u64) -> Result<u64, BlockError> {
        if lba >= self.partition.block_count {
            return Err(BlockError::InvalidAddress);
        }
        self.partition
            .start_lba
            .checked_add(lba)
            .ok_or(BlockError::InvalidAddress)
    }
}

impl BlockDevice for PartitionBlockDevice {
    fn block_size(&self) -> u32 {
        self.device.block_size()
    }

    fn block_count(&self) -> u64 {
        self.partition.block_count
    }

    fn name(&self) -> &str {
        &self.part_name
    }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() != self.block_size() as usize {
            return Err(BlockError::InvalidBufferSize);
        }
        let parent_lba = self.translate_lba(lba)?;
        self.device.read_block(parent_lba, buf)
    }

    fn write_block(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        if buf.len() != self.block_size() as usize {
            return Err(BlockError::InvalidBufferSize);
        }
        let parent_lba = self.translate_lba(lba)?;
        self.device.write_block(parent_lba, buf)
    }

    fn flush(&self) -> Result<(), BlockError> {
        self.device.flush()
    }

    fn is_read_only(&self) -> bool {
        self.device.is_read_only()
    }

    fn is_removable(&self) -> bool {
        self.device.is_removable()
    }
}

pub struct PartitionedDevice {
    device: Arc<dyn BlockDevice>,
    table_kind: PartitionTableKind,
    partitions: Vec<Partition>,
}

impl PartitionedDevice {
    pub fn table_kind(&self) -> PartitionTableKind {
        self.table_kind
    }

    pub fn device(&self) -> Arc<dyn BlockDevice> {
        self.device.clone()
    }

    pub fn partitions(&self) -> &[Partition] {
        &self.partitions
    }

    pub fn partition(&self, index: u32) -> Option<&Partition> {
        self.partitions.iter().find(|p| p.index == index)
    }

    pub fn open_partition(&self, index: u32) -> Result<Arc<dyn BlockDevice>, PartitionError> {
        let partition = self
            .partition(index)
            .copied()
            .ok_or(PartitionError::OutOfRange)?;
        Ok(Arc::new(PartitionBlockDevice::new(self.device.clone(), partition)))
    }

    fn new(device: Arc<dyn BlockDevice>, table_kind: PartitionTableKind, partitions: Vec<Partition>) -> Self {
        Self {
            device,
            table_kind,
            partitions,
        }
    }
}

/// Detect partition table and return parsed partitions.
pub fn discover_partitions(device: Arc<dyn BlockDevice>) -> Result<PartitionedDevice, PartitionError> {
    match gpt::parse_gpt_partitions(device.clone())? {
        Some(partitions) => {
            return Ok(PartitionedDevice::new(
                device,
                PartitionTableKind::Gpt,
                partitions,
            ));
        }
        None => {}
    }

    match mbr::parse_mbr_partitions(device.clone())? {
        Some(partitions) => Ok(PartitionedDevice::new(device, PartitionTableKind::Mbr, partitions)),
        None => Err(PartitionError::NoPartitionTable),
    }
}
