use alloc::string::String;
use alloc::sync::Arc;

use super::{BlockDevice, BlockError};
use crate::fs::vfs::{self, Inode};

enum FileBacking {
    Static(&'static [u8]),
    Vfs {
        inode: Arc<dyn Inode>,
        size: usize,
    },
}

pub struct ReadonlyFileBlockDevice {
    name: String,
    block_size: u32,
    backing: FileBacking,
}

impl ReadonlyFileBlockDevice {
    pub fn from_initramfs(
        name: &str,
        data: &'static [u8],
        block_size: u32,
    ) -> Result<Self, BlockError> {
        if block_size == 0 || data.is_empty() {
            return Err(BlockError::InvalidBufferSize);
        }
        if data.len() % block_size as usize != 0 {
            return Err(BlockError::InvalidBufferSize);
        }

        Ok(Self {
            name: String::from(name),
            block_size,
            backing: FileBacking::Static(data),
        })
    }

    pub fn from_inode(
        name: &str,
        inode: Arc<dyn Inode>,
        size: usize,
        block_size: u32,
    ) -> Result<Self, BlockError> {
        if block_size == 0 || size == 0 {
            return Err(BlockError::InvalidBufferSize);
        }
        if size % block_size as usize != 0 {
            return Err(BlockError::InvalidBufferSize);
        }
        let meta = inode.metadata().map_err(|_| BlockError::IoError)?;
        if meta.file_type != vfs::FileType::Regular {
            return Err(BlockError::InvalidAddress);
        }

        Ok(Self {
            name: String::from(name),
            block_size,
            backing: FileBacking::Vfs { inode, size },
        })
    }
}

impl BlockDevice for ReadonlyFileBlockDevice {
    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        match &self.backing {
            FileBacking::Static(data) => (data.len() / self.block_size as usize) as u64,
            FileBacking::Vfs { size, .. } => (*size / self.block_size as usize) as u64,
        }
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() != self.block_size as usize {
            return Err(BlockError::InvalidBufferSize);
        }
        let start = (lba as usize)
            .checked_mul(self.block_size as usize)
            .ok_or(BlockError::InvalidAddress)?;
        let end = start
            .checked_add(self.block_size as usize)
            .ok_or(BlockError::InvalidAddress)?;
        match &self.backing {
            FileBacking::Static(data) => {
                if end > data.len() {
                    return Err(BlockError::InvalidAddress);
                }
                buf.copy_from_slice(&data[start..end]);
                Ok(())
            }
            FileBacking::Vfs { inode, size } => {
                if end > *size {
                    return Err(BlockError::InvalidAddress);
                }
                let n = inode
                    .read_at(start, buf)
                    .map_err(|_| BlockError::IoError)?;
                if n != buf.len() {
                    return Err(BlockError::IoError);
                }
                Ok(())
            }
        }
    }

    fn write_block(&self, _lba: u64, _buf: &[u8]) -> Result<(), BlockError> {
        Err(BlockError::WriteProtected)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}
