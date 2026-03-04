//! Block Device Abstraction

use alloc::sync::Arc;
use core::fmt;

pub mod virtio_blk;

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
    fn flush(&self) -> Result<(), BlockError> { Ok(()) }
    fn is_read_only(&self) -> bool { false }
    fn is_removable(&self) -> bool { false }
}

/// Block device manager
pub struct BlockManager {
    devices: alloc::vec::Vec<Arc<dyn BlockDevice>>,
}

impl BlockManager {
    pub const fn new() -> Self {
        Self { devices: alloc::vec::Vec::new() }
    }

    pub fn register(&mut self, device: Arc<dyn BlockDevice>) -> usize {
        let index = self.devices.len();
        self.devices.push(device);
        index
    }

    pub fn count(&self) -> usize { self.devices.len() }
    pub fn get(&self, index: usize) -> Option<Arc<dyn BlockDevice>> { self.devices.get(index).cloned() }
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn BlockDevice>> { self.devices.iter() }
}

/// Initialize block device subsystem - registers drivers with PCI
pub fn init() {
    virtio_blk::init();
}
