use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::slice;

use crate::sync::Mutex;

use crate::fs::api::{DirEntry, FileType, FsError, Metadata};
use crate::fs::runtime::manager::FsStatFs;
use crate::fs::vfs::{FileSystem, Inode};

const CPIO_HEADER_LEN: usize = 110;
const CPIO_NEWC_MAGIC: &[u8; 6] = b"070701";
const CPIO_CRC_MAGIC: &[u8; 6] = b"070702";
const CPIO_TRAILER: &str = "TRAILER!!!";

#[derive(Clone)]
struct InitramfsEntry {
    path: String,
    ino: u64,
    mode: u32,
    kind: FileType,
    data: Option<&'static [u8]>,
}

struct InitramfsStore {
    entries: Vec<InitramfsEntry>,
}

impl InitramfsStore {
    const fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn find_entry(&self, path: &str) -> Option<&InitramfsEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    fn is_dir(&self, path: &str) -> bool {
        if path == "/" {
            return true;
        }

        if let Some(entry) = self.find_entry(path) {
            if entry.kind == FileType::Directory {
                return true;
            }
        }

        let mut prefix = String::from(path);
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        self.entries
            .iter()
            .any(|entry| entry.path.starts_with(prefix.as_str()))
    }

    fn read_file(&self, path: &str) -> Option<&'static [u8]> {
        self.find_entry(path)
            .and_then(|entry| (entry.kind == FileType::Regular).then_some(entry.data))
            .flatten()
    }

    fn directory_mode(&self, path: &str) -> u32 {
        self.find_entry(path)
            .and_then(|entry| (entry.kind == FileType::Directory).then_some(entry.mode))
            .unwrap_or(0o040755)
    }
}

static INITRAMFS: Mutex<InitramfsStore> = Mutex::new(InitramfsStore::empty());

#[inline]
fn align4(value: usize) -> usize {
    (value + 3) & !3
}

#[inline]
fn parse_hex_field(field: &[u8]) -> Result<u32, FsError> {
    let text = core::str::from_utf8(field).map_err(|_| FsError::InvalidInput)?;
    u32::from_str_radix(text, 16).map_err(|_| FsError::InvalidInput)
}

#[inline]
fn hash_path(path: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.as_bytes().iter().copied() {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn normalize_archive_path(raw: &str) -> Result<Option<String>, FsError> {
    if raw == CPIO_TRAILER {
        return Ok(None);
    }

    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return Ok(Some(String::from("/")));
    }

    let mut out = String::new();
    for comp in trimmed.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            return Err(FsError::InvalidInput);
        }
        out.push('/');
        out.push_str(comp);
    }

    if out.is_empty() {
        return Ok(Some(String::from("/")));
    }

    Ok(Some(out))
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

fn parse_archive(bytes: &'static [u8]) -> Result<Vec<InitramfsEntry>, FsError> {
    let mut cursor = 0usize;
    let mut by_path: BTreeMap<String, InitramfsEntry> = BTreeMap::new();

    while cursor.saturating_add(CPIO_HEADER_LEN) <= bytes.len() {
        let header = &bytes[cursor..cursor + CPIO_HEADER_LEN];
        let magic = &header[0..6];
        if magic != CPIO_NEWC_MAGIC && magic != CPIO_CRC_MAGIC {
            return Err(FsError::InvalidInput);
        }

        let ino = parse_hex_field(&header[6..14])? as u64;
        let mode = parse_hex_field(&header[14..22])?;
        let file_size = parse_hex_field(&header[54..62])? as usize;
        let name_size = parse_hex_field(&header[94..102])? as usize;
        if name_size == 0 {
            return Err(FsError::InvalidInput);
        }

        cursor = cursor.saturating_add(CPIO_HEADER_LEN);
        if cursor.saturating_add(name_size) > bytes.len() {
            return Err(FsError::InvalidInput);
        }

        let name_raw = &bytes[cursor..cursor + name_size];
        let nul_pos = name_raw
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(name_raw.len());
        let name = core::str::from_utf8(&name_raw[..nul_pos]).map_err(|_| FsError::InvalidInput)?;

        cursor = align4(cursor.saturating_add(name_size));
        if cursor.saturating_add(file_size) > bytes.len() {
            return Err(FsError::InvalidInput);
        }

        if name == CPIO_TRAILER {
            break;
        }

        let data = &bytes[cursor..cursor + file_size];
        cursor = align4(cursor.saturating_add(file_size));

        let Some(path) = normalize_archive_path(name)? else {
            continue;
        };
        if path == "/" {
            continue;
        }

        let kind = match mode & 0o170000 {
            0o040000 => FileType::Directory,
            0o100000 => FileType::Regular,
            _ => continue,
        };

        let entry = InitramfsEntry {
            path: path.clone(),
            ino: if ino != 0 {
                ino
            } else {
                hash_path(path.as_str())
            },
            mode: if mode != 0 {
                mode
            } else {
                match kind {
                    FileType::Directory => 0o040755,
                    FileType::Regular => 0o100644,
                    _ => 0,
                }
            },
            kind,
            data: if kind == FileType::Regular {
                Some(data)
            } else {
                None
            },
        };
        by_path.insert(path, entry);
    }

    Ok(by_path.into_values().collect())
}

pub fn init_from_phys(phys_addr: u64, size: u64) -> Result<(), FsError> {
    if size == 0 {
        INITRAMFS.lock().entries.clear();
        return Ok(());
    }

    let virt = crate::mm::phys_to_virt(phys_addr);
    if virt == 0 {
        return Err(FsError::InvalidInput);
    }

    let archive = unsafe { slice::from_raw_parts(virt as *const u8, size as usize) };
    let entries = parse_archive(archive)?;
    INITRAMFS.lock().entries = entries;
    Ok(())
}

pub fn read_file(path: &str) -> Option<&'static [u8]> {
    INITRAMFS.lock().read_file(path)
}

fn collect_dir_entries(dir_path: &str) -> Vec<DirEntry> {
    let store = INITRAMFS.lock();
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

    let mut prefix = String::from(dir_path);
    if !prefix.ends_with('/') {
        prefix.push('/');
    }

    for entry in store.entries.iter() {
        let path = entry.path.as_str();
        let rest = if dir_path == "/" {
            path.trim_start_matches('/')
        } else {
            let Some(stripped) = path.strip_prefix(prefix.as_str()) else {
                continue;
            };
            stripped
        };

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

        let child_kind = if tail.is_empty() {
            entry.kind
        } else {
            FileType::Directory
        };
        let child_ino = if tail.is_empty() {
            entry.ino
        } else {
            let mut full = String::from(prefix.as_str());
            full.push_str(name);
            hash_path(full.as_str())
        };

        names
            .entry(name.to_string())
            .and_modify(|slot| {
                if slot.0 != FileType::Directory && child_kind == FileType::Directory {
                    *slot = (child_kind, child_ino);
                }
            })
            .or_insert((child_kind, child_ino));
    }

    names
        .into_iter()
        .map(|(name, (file_type, ino))| DirEntry {
            ino,
            file_type,
            name,
        })
        .collect()
}

pub struct InitramfsFileSystem;

impl InitramfsFileSystem {
    pub const fn new() -> Self {
        Self
    }
}

impl FileSystem for InitramfsFileSystem {
    fn name(&self) -> &str {
        "initramfs"
    }

    fn root(&self) -> Arc<dyn Inode> {
        Arc::new(InitramfsInode {
            path: String::from("/"),
        })
    }

    fn sync(&self) -> Result<(), FsError> {
        Ok(())
    }

    fn statfs(&self) -> Result<FsStatFs, FsError> {
        let store = INITRAMFS.lock();
        let files = store.entries.len() as u64;
        let blocks = store
            .entries
            .iter()
            .map(|entry| entry.data.map(|data| data.len() as u64).unwrap_or(0))
            .sum::<u64>()
            .saturating_add(4095)
            / 4096;
        Ok(FsStatFs {
            f_type: 0x8584_58f6,
            f_bsize: 4096,
            f_blocks: blocks,
            f_bfree: 0,
            f_bavail: 0,
            f_files: files,
            f_ffree: 0,
            f_namelen: 255,
            f_frsize: 4096,
            f_flags: 1,
        })
    }
}

#[derive(Clone)]
struct InitramfsInode {
    path: String,
}

impl InitramfsInode {
    fn is_dir(&self) -> bool {
        INITRAMFS.lock().is_dir(self.path.as_str())
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

impl Inode for InitramfsInode {
    fn metadata(&self) -> Result<Metadata, FsError> {
        let store = INITRAMFS.lock();
        if let Some(entry) = store.find_entry(self.path.as_str()) {
            return Ok(Metadata {
                ino: entry.ino,
                file_type: entry.kind,
                mode: entry.mode,
                size: entry.data.map(|d| d.len() as u64).unwrap_or(0),
                nlink: if entry.kind == FileType::Directory {
                    2
                } else {
                    1
                },
            });
        }

        if store.is_dir(self.path.as_str()) {
            return Ok(Metadata {
                ino: hash_path(self.path.as_str()),
                file_type: FileType::Directory,
                mode: store.directory_mode(self.path.as_str()),
                size: 0,
                nlink: 2,
            });
        }

        Err(FsError::NotFound)
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        if !self.is_dir() {
            return Err(FsError::NotDirectory);
        }

        if name.is_empty() || name == "." {
            return Ok(Arc::new(self.clone()));
        }

        if name == ".." {
            let parent = if self.path == "/" {
                String::from("/")
            } else {
                String::from(split_parent(self.path.as_str()).0)
            };
            return Ok(Arc::new(InitramfsInode { path: parent }));
        }

        let child = self.join_child(name);
        let store = INITRAMFS.lock();
        if store.find_entry(child.as_str()).is_some() || store.is_dir(child.as_str()) {
            return Ok(Arc::new(InitramfsInode { path: child }));
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
        let store = INITRAMFS.lock();
        let Some(data) = store.read_file(self.path.as_str()) else {
            return Err(FsError::IsDirectory);
        };
        if offset >= data.len() || out.is_empty() {
            return Ok(0);
        }

        let n = out.len().min(data.len() - offset);
        out[..n].copy_from_slice(&data[offset..offset + n]);
        Ok(n)
    }
}
