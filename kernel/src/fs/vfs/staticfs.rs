use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::sync::Mutex;

use super::{DirEntry, FileSystem, FileType, FsError, Inode, Metadata};

#[derive(Clone, Copy)]
struct StaticFile {
    path: &'static str,
    data: &'static [u8],
}

static STATIC_FILES: Mutex<Vec<StaticFile>> = Mutex::new(Vec::new());

#[inline]
fn hash_path(path: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.as_bytes().iter().copied() {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn split_parent(path: &str) -> (&str, &str) {
    if path == "/" {
        return ("/", "");
    }
    let trimmed = path.trim_end_matches('/');
    if let Some(idx) = trimmed.rfind('/') {
        if idx == 0 {
            return ("/", &trimmed[1..]);
        }
        (&trimmed[..idx], &trimmed[idx + 1..])
    } else {
        ("/", trimmed)
    }
}

pub fn register_file(path: &'static str, data: &'static [u8]) -> Result<(), FsError> {
    if path.is_empty() || !path.starts_with('/') {
        return Err(FsError::InvalidInput);
    }
    let mut files = STATIC_FILES.lock();
    if let Some(existing) = files.iter_mut().find(|f| f.path == path) {
        existing.data = data;
        return Ok(());
    }
    files.push(StaticFile { path, data });
    Ok(())
}

pub fn read_file(path: &str) -> Option<&'static [u8]> {
    STATIC_FILES
        .lock()
        .iter()
        .find_map(|file| (file.path == path).then_some(file.data))
}

fn dir_exists(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    let needle = if path.ends_with('/') {
        String::from(path)
    } else {
        let mut s = String::from(path);
        s.push('/');
        s
    };
    STATIC_FILES
        .lock()
        .iter()
        .any(|f| f.path.starts_with(needle.as_str()))
}

fn collect_dir_entries(dir_path: &str) -> Vec<DirEntry> {
    let files = STATIC_FILES.lock();
    let mut names: BTreeMap<String, (FileType, u64)> = BTreeMap::new();
    names.insert(
        String::from("."),
        (FileType::Directory, hash_path(dir_path)),
    );
    let parent = if dir_path == "/" {
        "/"
    } else {
        split_parent(dir_path).0
    };
    names.insert(String::from(".."), (FileType::Directory, hash_path(parent)));

    for file in files.iter() {
        let path = file.path;
        if dir_path == "/" {
            let rest = path.trim_start_matches('/');
            if rest.is_empty() {
                continue;
            }
            let (name, tail) = match rest.split_once('/') {
                Some((a, b)) => (a, b),
                None => (rest, ""),
            };
            if name.is_empty() {
                continue;
            }
            let is_dir = !tail.is_empty();
            let entry_type = if is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            };
            let full = if is_dir {
                let mut s = String::from("/");
                s.push_str(name);
                s
            } else {
                String::from(path)
            };
            names
                .entry(String::from(name))
                .and_modify(|slot| {
                    if slot.0 != FileType::Directory && entry_type == FileType::Directory {
                        *slot = (entry_type, hash_path(full.as_str()));
                    }
                })
                .or_insert((entry_type, hash_path(full.as_str())));
        } else {
            let mut prefix = String::from(dir_path);
            if !prefix.ends_with('/') {
                prefix.push('/');
            }
            if !path.starts_with(prefix.as_str()) {
                continue;
            }
            let rest = &path[prefix.len()..];
            if rest.is_empty() {
                continue;
            }
            let (name, tail) = match rest.split_once('/') {
                Some((a, b)) => (a, b),
                None => (rest, ""),
            };
            if name.is_empty() {
                continue;
            }
            let is_dir = !tail.is_empty();
            let entry_type = if is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            };
            let full = if is_dir {
                let mut s = String::from(prefix.as_str());
                s.push_str(name);
                s
            } else {
                String::from(path)
            };
            names
                .entry(String::from(name))
                .and_modify(|slot| {
                    if slot.0 != FileType::Directory && entry_type == FileType::Directory {
                        *slot = (entry_type, hash_path(full.as_str()));
                    }
                })
                .or_insert((entry_type, hash_path(full.as_str())));
        }
    }

    let mut out = Vec::new();
    for (name, (file_type, ino)) in names {
        out.push(DirEntry {
            ino,
            file_type,
            name,
        });
    }
    out
}

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
        dir_exists(self.path.as_str())
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
                ino: hash_path(self.path.as_str()),
                file_type: FileType::Directory,
                mode: 0o040755,
                size: 0,
                nlink: 2,
            });
        }
        let Some(data) = read_file(self.path.as_str()) else {
            return Err(FsError::NotFound);
        };
        Ok(Metadata {
            ino: hash_path(self.path.as_str()),
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
            let parent = super::split_parent(self.path.as_str()).0;
            return Ok(Arc::new(StaticInode {
                path: String::from(parent),
            }));
        }
        let child = self.join_child(name);
        if dir_exists(child.as_str()) || read_file(child.as_str()).is_some() {
            return Ok(Arc::new(StaticInode { path: child }));
        }
        Err(FsError::NotFound)
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, FsError> {
        if !self.is_dir() {
            return Err(FsError::NotDirectory);
        }
        Ok(collect_dir_entries(self.path.as_str()))
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError> {
        if self.is_dir() {
            return Err(FsError::IsDirectory);
        }
        let Some(data) = read_file(self.path.as_str()) else {
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
