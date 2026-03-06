use alloc::sync::Arc;

use super::inode::Inode;
use crate::fs::api::FsError;

pub trait FileSystem: Send + Sync {
    fn name(&self) -> &str;
    fn root(&self) -> Arc<dyn Inode>;
    fn sync(&self) -> Result<(), FsError>;
}
