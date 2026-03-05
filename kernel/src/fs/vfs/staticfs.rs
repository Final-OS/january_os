use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::{DirEntry, FileSystem, FileType, FsError, Inode, Metadata};

pub struct StaticFileSystem;

impl StaticFileSystem {
    pub const fn new() -> Self {
        Self
    }
}

impl FileSystem for StaticFileSystem {
    fn name(&self) -> &str {
        "staticfs"
    }

    fn root(&self) -> Arc<dyn Inode> {
        Arc::new(StaticInode {
            path: String::from("/"),
        })
    }

    fn sync(&self) -> Result<(), FsError> {
        Ok(())
    }
}

#[derive(Clone)]
struct StaticInode {
    path: String,
}

impl StaticInode {
    fn is_dir(&self) -> bool {
        crate::fs::backend_dir_exists(self.path.as_str())
    }

    fn join_child(&self, name: &str) -> String {
        if self.path == "/" {
            let mut out = String::from("/");
            out.push_str(name);
            out
        } else {
            let mut out = self.path.clone();
            if !out.ends_with('/') {
                out.push('/');
            }
            out.push_str(name);
            out
        }
    }
}

impl Inode for StaticInode {
    fn metadata(&self) -> Result<Metadata, FsError> {
        if self.is_dir() {
            return Ok(Metadata {
                ino: crate::fs::backend_hash_path(self.path.as_str()),
                file_type: FileType::Directory,
                mode: 0o040755,
                size: 0,
                nlink: 2,
            });
        }
        let Some(data) = crate::fs::backend_read_static_file(self.path.as_str()) else {
            return Err(FsError::NotFound);
        };
        Ok(Metadata {
            ino: crate::fs::backend_hash_path(self.path.as_str()),
            file_type: FileType::Regular,
            mode: 0o100644,
            size: data.len() as u64,
            nlink: 1,
        })
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        if !self.is_dir() {
            return Err(FsError::NotDirectory);
        }
        if name.is_empty() || name == "." {
            return Ok(Arc::new(self.clone()));
        }
        if name == ".." {
            let parent = crate::fs::vfs::split_parent(self.path.as_str()).0;
            return Ok(Arc::new(StaticInode {
                path: String::from(parent),
            }));
        }
        let child = self.join_child(name);
        if crate::fs::backend_dir_exists(child.as_str())
            || crate::fs::backend_read_static_file(child.as_str()).is_some()
        {
            return Ok(Arc::new(StaticInode { path: child }));
        }
        Err(FsError::NotFound)
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, FsError> {
        if !self.is_dir() {
            return Err(FsError::NotDirectory);
        }
        let mut out = Vec::new();
        for entry in crate::fs::backend_collect_dir_entries(self.path.as_str()) {
            let file_type = match entry.file_type {
                crate::fs::BACKEND_DT_DIR => FileType::Directory,
                crate::fs::BACKEND_DT_REG => FileType::Regular,
                _ => FileType::Unknown,
            };
            out.push(DirEntry {
                ino: entry.ino,
                file_type,
                name: entry.name,
            });
        }
        Ok(out)
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError> {
        if self.is_dir() {
            return Err(FsError::IsDirectory);
        }
        let Some(data) = crate::fs::backend_read_static_file(self.path.as_str()) else {
            return Err(FsError::NotFound);
        };
        if offset >= data.len() || out.is_empty() {
            return Ok(0);
        }
        let n = out.len().min(data.len() - offset);
        out[..n].copy_from_slice(&data[offset..offset + n]);
        Ok(n)
    }
}
