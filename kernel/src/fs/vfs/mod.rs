//! Minimal VFS core interfaces for v0.3 bring-up.

pub mod staticfs;

mod file;
mod fs;
mod inode;
mod mount;
mod path;
mod types;

use alloc::sync::Arc;

pub use file::File;
pub use fs::FileSystem;
pub use inode::Inode;
pub use mount::{MountEntry, mount_fs, mount_root, mount_snapshot, resolve_mount, umount_fs};
pub use path::{normalize_path, split_parent};
pub use types::{DirEntry, FileType, FsError, Metadata, SeekWhence};

pub fn lookup_path(path: &str) -> Result<Arc<dyn Inode>, FsError> {
    let (mount_target, fs) = resolve_mount(path).ok_or(FsError::NotFound)?;
    let mut inode = fs.root();
    let mut rel = path;
    if mount_target != "/" {
        rel = rel
            .strip_prefix(mount_target.as_str())
            .ok_or(FsError::NotFound)?;
        rel = rel.strip_prefix('/').unwrap_or(rel);
    } else {
        rel = rel.strip_prefix('/').unwrap_or(rel);
    }
    for comp in rel.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        inode = inode.lookup(comp)?;
    }
    Ok(inode)
}
