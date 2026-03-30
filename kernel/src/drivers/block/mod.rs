//! Block Device Abstraction

use alloc::sync::Arc;
use core::fmt;

pub mod file_backed;
pub mod gpt;
pub mod mbr;
pub mod partition;
pub mod virtio_blk;
pub mod virtio_scsi;
pub use file_backed::ReadonlyFileBlockDevice;
pub use partition::{
    Partition, PartitionBlockDevice, PartitionError, PartitionTableKind, PartitionType,
    PartitionedDevice, discover_partitions,
};

/// Block device error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    InvalidAddress,
    InvalidBufferSize,
    NotReady,
    IoError,
    WriteProtected,
    Timeout,
    Unsupported,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::InvalidAddress => write!(f, "Invalid block address"),
            BlockError::InvalidBufferSize => write!(f, "Invalid buffer size"),
            BlockError::NotReady => write!(f, "Device not ready"),
            BlockError::IoError => write!(f, "I/O error"),
            BlockError::WriteProtected => write!(f, "Write protected"),
            BlockError::Timeout => write!(f, "Timeout"),
            BlockError::Unsupported => write!(f, "Unsupported operation"),
        }
    }
}

/// Block device trait
pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> u32;
    fn block_count(&self) -> u64;
    fn name(&self) -> &str;
    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_block(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError>;
    fn flush(&self) -> Result<(), BlockError> {
        Ok(())
    }
    fn is_read_only(&self) -> bool {
        false
    }
    fn is_removable(&self) -> bool {
        false
    }
}

/// Block device manager
pub struct BlockManager {
    devices: alloc::vec::Vec<Arc<dyn BlockDevice>>,
}

pub struct StaticBlockDeviceRef {
    inner: &'static dyn BlockDevice,
}

impl StaticBlockDeviceRef {
    pub fn new(inner: &'static dyn BlockDevice) -> Self {
        Self { inner }
    }
}

impl BlockDevice for StaticBlockDeviceRef {
    fn block_size(&self) -> u32 {
        self.inner.block_size()
    }

    fn block_count(&self) -> u64 {
        self.inner.block_count()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        self.inner.read_block(lba, buf)
    }

    fn write_block(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        self.inner.write_block(lba, buf)
    }

    fn flush(&self) -> Result<(), BlockError> {
        self.inner.flush()
    }

    fn is_read_only(&self) -> bool {
        self.inner.is_read_only()
    }

    fn is_removable(&self) -> bool {
        self.inner.is_removable()
    }
}

impl BlockManager {
    pub const fn new() -> Self {
        Self {
            devices: alloc::vec::Vec::new(),
        }
    }

    pub fn register(&mut self, device: Arc<dyn BlockDevice>) -> usize {
        let index = self.devices.len();
        self.devices.push(device);
        index
    }

    pub fn count(&self) -> usize {
        self.devices.len()
    }
    pub fn get(&self, index: usize) -> Option<Arc<dyn BlockDevice>> {
        self.devices.get(index).cloned()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn BlockDevice>> {
        self.devices.iter()
    }
}

/// Initialize block device subsystem - registers drivers with PCI
pub fn init() {
    virtio_blk::init();
    virtio_scsi::init();
}

pub fn boot_device() -> Option<Arc<dyn BlockDevice>> {
    if let Some(dev) = virtio_blk::get_device() {
        return Some(Arc::new(StaticBlockDeviceRef::new(
            dev as &'static dyn BlockDevice,
        )));
    }
    if let Some(dev) = virtio_scsi::get_device() {
        return Some(Arc::new(StaticBlockDeviceRef::new(
            dev as &'static dyn BlockDevice,
        )));
    }
    None
}
