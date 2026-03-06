use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::fs::api::{DirEntry, FsError, Metadata};

pub trait Inode: Send + Sync {
    fn metadata(&self) -> Result<Metadata, FsError>;
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError>;
    fn readdir(&self) -> Result<Vec<DirEntry>, FsError>;
    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError>;
}
