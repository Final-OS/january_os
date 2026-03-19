use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::sync::Mutex;

use super::filesystem::FileSystem;
use super::path::normalize_path;
use crate::fs::api::FsError;

#[derive(Clone)]
pub struct MountEntry {
    pub target: String,
    pub fs_name: String,
    pub source: Option<String>,
}

struct MountRecord {
    fs: Arc<dyn FileSystem>,
    source: Option<String>,
}

struct MountTable {
    mounts: BTreeMap<String, MountRecord>,
}

impl MountTable {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }

    fn mount_root(&mut self, fs: Arc<dyn FileSystem>) {
        self.mounts.insert(
            String::from("/"),
            MountRecord { fs, source: None },
        );
    }

    fn mount(
        &mut self,
        target: &str,
        fs: Arc<dyn FileSystem>,
        source: Option<&str>,
    ) -> Result<(), FsError> {
        let normalized = normalize_path("/", target)?;
        if normalized == "/" {
            self.mount_root(fs);
            return Ok(());
        }
        if self.mounts.contains_key(&normalized) {
            return Err(FsError::Busy);
        }
        self.mounts.insert(
            normalized,
            MountRecord {
                fs,
                source: source.map(String::from),
            },
        );
        Ok(())
    }

    fn umount(&mut self, target: &str) -> Result<(), FsError> {
        let normalized = normalize_path("/", target)?;
        if normalized == "/" {
            return Err(FsError::Busy);
        }
        self.mounts
            .remove(&normalized)
            .map(|_| ())
            .ok_or(FsError::NotFound)
    }

    fn resolve(&self, path: &str) -> Option<(String, Arc<dyn FileSystem>)> {
        let normalized = normalize_path("/", path).ok()?;
        let mut best: Option<(&str, Arc<dyn FileSystem>)> = None;
        for (target, record) in self.mounts.iter() {
            if target == "/"
                || normalized == *target
                || (normalized.starts_with(target.as_str())
                    && normalized
                        .as_bytes()
                        .get(target.len())
                        .map(|b| *b == b'/')
                        .unwrap_or(false))
            {
                match best {
                    Some((old, _)) if old.len() >= target.len() => {}
                    _ => best = Some((target.as_str(), record.fs.clone())),
                }
            }
        }
        best.map(|(t, fs)| (String::from(t), fs))
    }

    fn snapshot(&self) -> Vec<MountEntry> {
        let mut out = Vec::new();
        for (target, record) in self.mounts.iter() {
            out.push(MountEntry {
                target: target.clone(),
                fs_name: String::from(record.fs.name()),
                source: record.source.clone(),
            });
        }
        out
    }
}

static MOUNT_TABLE: Mutex<MountTable> = Mutex::new(MountTable::new());

pub fn mount_root(fs: Arc<dyn FileSystem>) {
    MOUNT_TABLE.lock().mount_root(fs);
}

pub fn mount_fs(target: &str, fs: Arc<dyn FileSystem>) -> Result<(), FsError> {
    MOUNT_TABLE.lock().mount(target, fs, None)
}

pub fn mount_fs_with_source(
    target: &str,
    source: &str,
    fs: Arc<dyn FileSystem>,
) -> Result<(), FsError> {
    MOUNT_TABLE.lock().mount(target, fs, Some(source))
}

pub fn umount_fs(target: &str) -> Result<(), FsError> {
    MOUNT_TABLE.lock().umount(target)
}

pub fn resolve_mount(path: &str) -> Option<(String, Arc<dyn FileSystem>)> {
    MOUNT_TABLE.lock().resolve(path)
}

pub fn mount_snapshot() -> Vec<MountEntry> {
    MOUNT_TABLE.lock().snapshot()
}
