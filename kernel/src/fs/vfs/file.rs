use alloc::sync::Arc;

use super::inode::Inode;
use super::types::{FsError, SeekWhence};

pub trait File: Send + Sync {
    fn inode(&self) -> Arc<dyn Inode>;
    fn read(&mut self, out: &mut [u8]) -> Result<usize, FsError>;
    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError>;
    fn seek(&mut self, offset: i64, whence: SeekWhence) -> Result<usize, FsError>;
}
