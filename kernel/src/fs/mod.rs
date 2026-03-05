//! Minimal in-kernel file backend for v0.2 syscall bring-up.
//!
//! This module intentionally keeps scope small:
//! - static read-only files (registered at runtime)
//! - per-process fd table for `open/read/close`
//! - lookup API for `execve` image provider

pub mod vfs;

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::sync::Mutex;
use crate::syscall::{EAGAIN, EBADF, EINVAL, EISDIR, ENOENT, ENOTDIR, EPIPE, ESPIPE};

const FIRST_USER_FD: i32 = 3;
const O_CLOEXEC: u32 = 0o2000000;
const O_NONBLOCK: u32 = 0o4000;
const O_DIRECTORY: u32 = 0o200000;
const O_ACCMODE: u32 = 0o3;
const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;

#[derive(Clone, Copy)]
struct StaticFile {
    path: &'static str,
    data: &'static [u8],
}

#[derive(Clone)]
struct StaticOpenFile {
    path: &'static str,
    data: &'static [u8],
    offset: usize,
    cloexec: bool,
}

#[derive(Clone)]
struct DirOpenFile {
    path: String,
    cursor: usize,
    cloexec: bool,
    inode: Option<Arc<dyn vfs::Inode>>,
    ino: u64,
    mode: u32,
}

#[derive(Clone)]
struct VfsOpenFile {
    path: String,
    inode: Arc<dyn vfs::Inode>,
    ino: u64,
    mode: u32,
    static_data: Option<&'static [u8]>,
    offset: usize,
    cloexec: bool,
}

struct VfsOpenHint {
    inode: Arc<dyn vfs::Inode>,
    meta: vfs::Metadata,
    static_data: Option<&'static [u8]>,
}

#[derive(Clone)]
enum OpenFile {
    Static(StaticOpenFile),
    Vfs(VfsOpenFile),
    Dir(DirOpenFile),
    PipeRead {
        pipe_id: u64,
        nonblocking: bool,
        cloexec: bool,
    },
    PipeWrite {
        pipe_id: u64,
        nonblocking: bool,
        cloexec: bool,
    },
}

struct PipeState {
    data: VecDeque<u8>,
    readers: u32,
    writers: u32,
}

impl PipeState {
    fn new() -> Self {
        Self {
            data: VecDeque::new(),
            readers: 1,
            writers: 1,
        }
    }
}

struct FsState {
    files: Vec<StaticFile>,
    open_files: BTreeMap<usize, BTreeMap<i32, OpenFile>>,
    next_fd: BTreeMap<usize, i32>,
    cwd: BTreeMap<usize, String>,
    next_pipe_id: u64,
    pipes: BTreeMap<u64, PipeState>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FsStat {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u64,
    pub rdev: u64,
    pub size: i64,
    pub blksize: i64,
    pub blocks: i64,
}

const S_IFIFO: u32 = 0o010000;
const S_IFCHR: u32 = 0o020000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

const POLLIN: i16 = 0x0001;
const POLLOUT: i16 = 0x0004;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

const DT_UNKNOWN: u8 = 0;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;

#[derive(Clone)]
pub struct FsDirEntry {
    pub ino: u64,
    pub file_type: u8,
    pub name: String,
}

#[inline]
fn hash_path_impl(path: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.as_bytes().iter().copied() {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

impl FsState {
    const fn new() -> Self {
        Self {
            files: Vec::new(),
            open_files: BTreeMap::new(),
            next_fd: BTreeMap::new(),
            cwd: BTreeMap::new(),
            next_pipe_id: 1,
            pipes: BTreeMap::new(),
        }
    }

    fn alloc_fd(&mut self, pid: usize) -> i32 {
        self.alloc_fd_from(pid, FIRST_USER_FD)
    }

    fn alloc_fd_from(&mut self, pid: usize, min_fd: i32) -> i32 {
        let start = min_fd.max(FIRST_USER_FD);
        let next_fd = self.next_fd.entry(pid).or_insert(start);
        if *next_fd < start {
            *next_fd = start;
        }
        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        let mut fd = *next_fd;
        while table.contains_key(&fd) {
            fd = fd.saturating_add(1);
        }
        *next_fd = fd.saturating_add(1);
        fd
    }

    fn ensure_cwd_mut(&mut self, pid: usize) -> &mut String {
        self.cwd.entry(pid).or_insert_with(|| String::from("/"))
    }

    fn cwd_for_pid(&mut self, pid: usize) -> String {
        self.ensure_cwd_mut(pid).clone()
    }

    fn normalize_path(base: &str, input: &str) -> Result<String, i32> {
        if input.is_empty() {
            return Err(ENOENT);
        }
        let mut parts: Vec<&str> = Vec::new();
        let mut seed = String::from(base);
        if !seed.starts_with('/') {
            seed.insert(0, '/');
        }
        if !input.starts_with('/') {
            for comp in seed.split('/') {
                if comp.is_empty() || comp == "." {
                    continue;
                }
                if comp == ".." {
                    let _ = parts.pop();
                } else {
                    parts.push(comp);
                }
            }
        }
        for comp in input.split('/') {
            if comp.is_empty() || comp == "." {
                continue;
            }
            if comp == ".." {
                let _ = parts.pop();
            } else {
                parts.push(comp);
            }
        }
        if parts.is_empty() {
            return Ok(String::from("/"));
        }
        let mut out = String::from("/");
        for (idx, comp) in parts.iter().enumerate() {
            if idx != 0 {
                out.push('/');
            }
            out.push_str(comp);
        }
        Ok(out)
    }

    fn resolve_path_for_pid(&mut self, pid: usize, input: &str) -> Result<String, i32> {
        let cwd = self.cwd_for_pid(pid);
        Self::normalize_path(cwd.as_str(), input)
    }

    fn dir_exists(&self, path: &str) -> bool {
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
        self.files.iter().any(|f| f.path.starts_with(needle.as_str()))
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

    fn collect_dir_entries(&self, dir_path: &str) -> Vec<FsDirEntry> {
        let mut names: BTreeMap<String, (u8, u64)> = BTreeMap::new();
        let dir_ino = Self::hash_path(dir_path);
        names.insert(String::from("."), (DT_DIR, dir_ino));
        let parent = if dir_path == "/" {
            "/"
        } else {
            Self::split_parent(dir_path).0
        };
        names.insert(String::from(".."), (DT_DIR, Self::hash_path(parent)));

        for file in self.files.iter() {
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
                let entry_type = if is_dir { DT_DIR } else { DT_REG };
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
                        if slot.0 != DT_DIR && entry_type == DT_DIR {
                            *slot = (entry_type, Self::hash_path(full.as_str()));
                        }
                    })
                    .or_insert((entry_type, Self::hash_path(full.as_str())));
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
                let entry_type = if is_dir { DT_DIR } else { DT_REG };
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
                        if slot.0 != DT_DIR && entry_type == DT_DIR {
                            *slot = (entry_type, Self::hash_path(full.as_str()));
                        }
                    })
                    .or_insert((entry_type, Self::hash_path(full.as_str())));
            }
        }

        let mut out = Vec::new();
        for (name, (file_type, ino)) in names {
            out.push(FsDirEntry {
                ino,
                file_type,
                name,
            });
        }
        out
    }

    fn register_static_file(&mut self, path: &'static str, data: &'static [u8]) -> Result<(), i32> {
        if path.is_empty() || !path.starts_with('/') {
            return Err(EINVAL);
        }

        if let Some(existing) = self.files.iter_mut().find(|f| f.path == path) {
            existing.data = data;
            return Ok(());
        }

        self.files.push(StaticFile { path, data });
        Ok(())
    }

    fn read_static_file(&self, path: &str) -> Option<&'static [u8]> {
        self.files
            .iter()
            .find_map(|file| (file.path == path).then_some(file.data))
    }

    fn open_for_pid(
        &mut self,
        pid: usize,
        path: &str,
        flags: u32,
        _mode: u16,
        vfs_hint: Option<VfsOpenHint>,
    ) -> Result<i32, i32> {
        let resolved = self.resolve_path_for_pid(pid, path)?;
        let cloexec = (flags & O_CLOEXEC) != 0;
        let wants_dir = (flags & O_DIRECTORY) != 0;
        let accmode = flags & O_ACCMODE;
        if accmode != O_RDONLY && accmode != O_WRONLY {
            return Err(EINVAL);
        }

        let Some(hint) = vfs_hint else {
            return Err(ENOENT);
        };

        let open_file = match hint.meta.file_type {
            vfs::FileType::Directory => {
                if accmode != O_RDONLY {
                    return Err(EISDIR);
                }
                OpenFile::Dir(DirOpenFile {
                    path: resolved,
                    cursor: 0,
                    cloexec,
                    inode: Some(hint.inode),
                    ino: hint.meta.ino,
                    mode: hint.meta.mode,
                })
            }
            vfs::FileType::Regular => {
                if wants_dir {
                    return Err(ENOTDIR);
                }
                OpenFile::Vfs(VfsOpenFile {
                    path: resolved,
                    inode: hint.inode,
                    ino: hint.meta.ino,
                    mode: hint.meta.mode,
                    static_data: hint.static_data,
                    offset: 0,
                    cloexec,
                })
            }
            _ => return Err(ENOENT),
        };

        let fd = self.alloc_fd(pid);
        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(fd, open_file);
        Ok(fd)
    }

    fn pipe2_for_pid(&mut self, pid: usize, flags: u32) -> Result<(i32, i32), i32> {
        let allowed = O_NONBLOCK | O_CLOEXEC;
        if (flags & !allowed) != 0 {
            return Err(EINVAL);
        }
        let nonblocking = (flags & O_NONBLOCK) != 0;
        let cloexec = (flags & O_CLOEXEC) != 0;

        let pipe_id = self.next_pipe_id;
        self.next_pipe_id = self.next_pipe_id.saturating_add(1);

        let read_fd = self.alloc_fd(pid);
        let write_fd = self.alloc_fd(pid);

        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(
            read_fd,
            OpenFile::PipeRead {
                pipe_id,
                nonblocking,
                cloexec,
            },
        );
        table.insert(
            write_fd,
            OpenFile::PipeWrite {
                pipe_id,
                nonblocking,
                cloexec,
            },
        );
        self.pipes.insert(pipe_id, PipeState::new());

        Ok((read_fd, write_fd))
    }

    fn read_for_pid(&mut self, pid: usize, fd: i32, out: &mut [u8]) -> Result<usize, i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd).cloned() else {
            return Err(EBADF);
        };

        if out.is_empty() {
            return Ok(0);
        }

        match open_file {
            OpenFile::Static(_) => {
                let Some(table) = self.open_files.get_mut(&pid) else {
                    return Err(EBADF);
                };
                let Some(OpenFile::Static(open_file)) = table.get_mut(&fd) else {
                    return Err(EBADF);
                };

                let remain = open_file.data.len().saturating_sub(open_file.offset);
                let read_len = remain.min(out.len());
                let end = open_file.offset.saturating_add(read_len);
                out[..read_len].copy_from_slice(&open_file.data[open_file.offset..end]);
                open_file.offset = end;
                Ok(read_len)
            }
            OpenFile::Vfs(_) => {
                let Some(table) = self.open_files.get_mut(&pid) else {
                    return Err(EBADF);
                };
                let Some(OpenFile::Vfs(open_file)) = table.get_mut(&fd) else {
                    return Err(EBADF);
                };
                let read_len = if let Some(data) = open_file.static_data {
                    if open_file.offset >= data.len() {
                        0
                    } else {
                        let n = out.len().min(data.len() - open_file.offset);
                        out[..n].copy_from_slice(&data[open_file.offset..open_file.offset + n]);
                        n
                    }
                } else {
                    open_file
                        .inode
                        .read_at(open_file.offset, out)
                        .map_err(|e| e.errno())?
                };
                open_file.offset = open_file.offset.saturating_add(read_len);
                Ok(read_len)
            }
            OpenFile::PipeRead {
                pipe_id,
                nonblocking,
                ..
            } => {
                let Some(pipe) = self.pipes.get_mut(&pipe_id) else {
                    return Err(EBADF);
                };

                let mut read_len = 0usize;
                while read_len < out.len() {
                    let Some(byte) = pipe.data.pop_front() else {
                        break;
                    };
                    out[read_len] = byte;
                    read_len += 1;
                }

                if read_len > 0 {
                    return Ok(read_len);
                }

                if pipe.writers == 0 {
                    return Ok(0);
                }
                if nonblocking {
                    return Err(EAGAIN);
                }
                Err(EAGAIN)
            }
            OpenFile::Dir(_) => Err(EISDIR),
            OpenFile::PipeWrite { .. } => Err(EBADF),
        }
    }

    fn write_for_pid(&mut self, pid: usize, fd: i32, data: &[u8]) -> Result<usize, i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd).cloned() else {
            return Err(EBADF);
        };

        if data.is_empty() {
            return Ok(0);
        }

        match open_file {
            OpenFile::PipeWrite { pipe_id, .. } => {
                let Some(pipe) = self.pipes.get_mut(&pipe_id) else {
                    return Err(EBADF);
                };
                if pipe.readers == 0 {
                    return Err(EPIPE);
                }
                for byte in data.iter().copied() {
                    pipe.data.push_back(byte);
                }
                Ok(data.len())
            }
            OpenFile::Dir(_) => Err(EISDIR),
            _ => Err(EBADF),
        }
    }

    fn close_for_pid(&mut self, pid: usize, fd: i32) -> Result<(), i32> {
        let Some(table) = self.open_files.get_mut(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.remove(&fd) else {
            return Err(EBADF);
        };

        match open_file {
            OpenFile::PipeRead { pipe_id, .. } => {
                if let Some(pipe) = self.pipes.get_mut(&pipe_id) {
                    pipe.readers = pipe.readers.saturating_sub(1);
                    if pipe.readers == 0 && pipe.writers == 0 {
                        self.pipes.remove(&pipe_id);
                    }
                }
            }
            OpenFile::PipeWrite { pipe_id, .. } => {
                if let Some(pipe) = self.pipes.get_mut(&pipe_id) {
                    pipe.writers = pipe.writers.saturating_sub(1);
                    if pipe.readers == 0 && pipe.writers == 0 {
                        self.pipes.remove(&pipe_id);
                    }
                }
            }
            OpenFile::Dir(_) => {}
            OpenFile::Static(_) => {}
            OpenFile::Vfs(_) => {}
        };
        Ok(())
    }

    fn read_at_for_pid(
        &self,
        pid: usize,
        fd: i32,
        offset: usize,
        out: &mut [u8],
    ) -> Result<usize, i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };
        if out.is_empty() {
            return Ok(0);
        }
        match open_file {
            OpenFile::Static(open_file) => {
                if offset >= open_file.data.len() {
                    return Ok(0);
                }
                let remain = open_file.data.len().saturating_sub(offset);
                let read_len = remain.min(out.len());
                out[..read_len]
                    .copy_from_slice(&open_file.data[offset..offset.saturating_add(read_len)]);
                Ok(read_len)
            }
            OpenFile::Vfs(open_file) if open_file.static_data.is_some() => {
                let data = open_file.static_data.unwrap_or(&[]);
                if offset >= data.len() {
                    return Ok(0);
                }
                let read_len = (data.len() - offset).min(out.len());
                out[..read_len].copy_from_slice(&data[offset..offset + read_len]);
                Ok(read_len)
            }
            OpenFile::Vfs(open_file) => open_file.inode.read_at(offset, out).map_err(|e| e.errno()),
            OpenFile::Dir(_) => Err(EISDIR),
            _ => Err(EBADF),
        }
    }

    fn mmap_file_for_pid(&self, pid: usize, fd: i32) -> Result<&'static [u8], i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };
        match open_file {
            OpenFile::Static(open_file) => Ok(open_file.data),
            OpenFile::Vfs(open_file) => open_file.static_data.ok_or(EBADF),
            OpenFile::Dir(_) => Err(EISDIR),
            _ => Err(EBADF),
        }
    }

    fn fd_is_nonblocking_for_pid(&self, pid: usize, fd: i32) -> Result<bool, i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };

        match open_file {
            OpenFile::Static(_) => Ok(false),
            OpenFile::Vfs(_) => Ok(false),
            OpenFile::Dir(_) => Ok(false),
            OpenFile::PipeRead { nonblocking, .. } => Ok(*nonblocking),
            OpenFile::PipeWrite { nonblocking, .. } => Ok(*nonblocking),
        }
    }

    fn lseek_for_pid(&mut self, pid: usize, fd: i32, offset: i64, whence: u32) -> Result<usize, i32> {
        let Some(table) = self.open_files.get_mut(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get_mut(&fd) else {
            return Err(EBADF);
        };

        match open_file {
            OpenFile::Static(file) => {
                let base = match whence {
                    0 => 0i64,                   // SEEK_SET
                    1 => file.offset as i64,     // SEEK_CUR
                    2 => file.data.len() as i64, // SEEK_END
                    _ => return Err(EINVAL),
                };
                let next = base.checked_add(offset).ok_or(EINVAL)?;
                if next < 0 {
                    return Err(EINVAL);
                }
                file.offset = next as usize;
                Ok(file.offset)
            }
            OpenFile::Vfs(file) => {
                let size = file
                    .static_data
                    .map(|data| data.len() as i64)
                    .unwrap_or_else(|| file.inode.metadata().map(|m| m.size as i64).unwrap_or(0));
                let base = match whence {
                    0 => 0i64,               // SEEK_SET
                    1 => file.offset as i64, // SEEK_CUR
                    2 => size,               // SEEK_END
                    _ => return Err(EINVAL),
                };
                let next = base.checked_add(offset).ok_or(EINVAL)?;
                if next < 0 {
                    return Err(EINVAL);
                }
                file.offset = next as usize;
                Ok(file.offset)
            }
            OpenFile::Dir(dir) => {
                if whence != 0 || offset < 0 {
                    return Err(EINVAL);
                }
                dir.cursor = offset as usize;
                Ok(dir.cursor)
            }
            OpenFile::PipeRead { .. } | OpenFile::PipeWrite { .. } => Err(ESPIPE),
        }
    }

    fn duplicate_open_file(&mut self, open_file: &OpenFile) {
        match open_file {
            OpenFile::PipeRead { pipe_id, .. } => {
                if let Some(pipe) = self.pipes.get_mut(pipe_id) {
                    pipe.readers = pipe.readers.saturating_add(1);
                }
            }
            OpenFile::PipeWrite { pipe_id, .. } => {
                if let Some(pipe) = self.pipes.get_mut(pipe_id) {
                    pipe.writers = pipe.writers.saturating_add(1);
                }
            }
            OpenFile::Static(_) | OpenFile::Vfs(_) | OpenFile::Dir(_) => {}
        }
    }

    fn dup_for_pid(&mut self, pid: usize, oldfd: i32, min_newfd: i32, cloexec: bool) -> Result<i32, i32> {
        let old_file = {
            let Some(table) = self.open_files.get(&pid) else {
                return Err(EBADF);
            };
            let Some(entry) = table.get(&oldfd) else {
                return Err(EBADF);
            };
            entry.clone()
        };

        let mut new_file = old_file.clone();
        match &mut new_file {
            OpenFile::Static(f) => f.cloexec = cloexec,
            OpenFile::Vfs(f) => f.cloexec = cloexec,
            OpenFile::Dir(d) => d.cloexec = cloexec,
            OpenFile::PipeRead { cloexec: c, .. } => *c = cloexec,
            OpenFile::PipeWrite { cloexec: c, .. } => *c = cloexec,
        }
        self.duplicate_open_file(&new_file);
        let new_fd = self.alloc_fd_from(pid, min_newfd);
        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(new_fd, new_file);
        Ok(new_fd)
    }

    fn dup2_for_pid(&mut self, pid: usize, oldfd: i32, newfd: i32, cloexec: bool) -> Result<i32, i32> {
        if newfd < 0 {
            return Err(EBADF);
        }
        if oldfd == newfd {
            let table = self.open_files.get(&pid).ok_or(EBADF)?;
            if table.get(&oldfd).is_none() {
                return Err(EBADF);
            }
            return Ok(newfd);
        }

        let old_file = {
            let Some(table) = self.open_files.get(&pid) else {
                return Err(EBADF);
            };
            let Some(entry) = table.get(&oldfd) else {
                return Err(EBADF);
            };
            entry.clone()
        };

        let _ = self.close_for_pid(pid, newfd);

        let mut new_file = old_file.clone();
        match &mut new_file {
            OpenFile::Static(f) => f.cloexec = cloexec,
            OpenFile::Vfs(f) => f.cloexec = cloexec,
            OpenFile::Dir(d) => d.cloexec = cloexec,
            OpenFile::PipeRead { cloexec: c, .. } => *c = cloexec,
            OpenFile::PipeWrite { cloexec: c, .. } => *c = cloexec,
        }
        self.duplicate_open_file(&new_file);
        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(newfd, new_file);
        let next_fd = self.next_fd.entry(pid).or_insert(FIRST_USER_FD);
        if *next_fd <= newfd {
            *next_fd = newfd.saturating_add(1);
        }
        Ok(newfd)
    }

    fn fcntl_getfl_for_pid(&self, pid: usize, fd: i32) -> Result<u32, i32> {
        let table = self.open_files.get(&pid).ok_or(EBADF)?;
        let open_file = table.get(&fd).ok_or(EBADF)?;
        Ok(match open_file {
            OpenFile::Static(_) | OpenFile::Vfs(_) | OpenFile::Dir(_) => O_RDONLY,
            OpenFile::PipeRead { nonblocking, .. } => {
                let mut f = O_RDONLY;
                if *nonblocking {
                    f |= O_NONBLOCK;
                }
                f
            }
            OpenFile::PipeWrite { nonblocking, .. } => {
                let mut f = O_WRONLY;
                if *nonblocking {
                    f |= O_NONBLOCK;
                }
                f
            }
        })
    }

    fn fcntl_setfl_for_pid(&mut self, pid: usize, fd: i32, flags: u32) -> Result<(), i32> {
        let table = self.open_files.get_mut(&pid).ok_or(EBADF)?;
        let open_file = table.get_mut(&fd).ok_or(EBADF)?;
        let nonblocking = (flags & O_NONBLOCK) != 0;
        match open_file {
            OpenFile::PipeRead { nonblocking: n, .. } => *n = nonblocking,
            OpenFile::PipeWrite { nonblocking: n, .. } => *n = nonblocking,
            OpenFile::Static(_) | OpenFile::Vfs(_) | OpenFile::Dir(_) => {}
        }
        Ok(())
    }

    fn fcntl_getfd_for_pid(&self, pid: usize, fd: i32) -> Result<u32, i32> {
        let table = self.open_files.get(&pid).ok_or(EBADF)?;
        let open_file = table.get(&fd).ok_or(EBADF)?;
        let cloexec = match open_file {
            OpenFile::Static(f) => f.cloexec,
            OpenFile::Vfs(f) => f.cloexec,
            OpenFile::Dir(f) => f.cloexec,
            OpenFile::PipeRead { cloexec, .. } => *cloexec,
            OpenFile::PipeWrite { cloexec, .. } => *cloexec,
        };
        Ok(if cloexec { 1 } else { 0 })
    }

    fn fcntl_setfd_for_pid(&mut self, pid: usize, fd: i32, cloexec: bool) -> Result<(), i32> {
        let table = self.open_files.get_mut(&pid).ok_or(EBADF)?;
        let open_file = table.get_mut(&fd).ok_or(EBADF)?;
        match open_file {
            OpenFile::Static(f) => f.cloexec = cloexec,
            OpenFile::Vfs(f) => f.cloexec = cloexec,
            OpenFile::Dir(f) => f.cloexec = cloexec,
            OpenFile::PipeRead { cloexec: c, .. } => *c = cloexec,
            OpenFile::PipeWrite { cloexec: c, .. } => *c = cloexec,
        }
        Ok(())
    }

    fn chdir_for_pid(&mut self, pid: usize, path: &str) -> Result<(), i32> {
        let resolved = self.resolve_path_for_pid(pid, path)?;
        let inode = vfs::lookup_path(resolved.as_str()).map_err(|e| e.errno())?;
        let meta = inode.metadata().map_err(|e| e.errno())?;
        if meta.file_type != vfs::FileType::Directory {
            return Err(ENOTDIR);
        }
        let cwd = self.ensure_cwd_mut(pid);
        *cwd = resolved;
        Ok(())
    }

    fn getcwd_for_pid(&mut self, pid: usize) -> String {
        self.cwd_for_pid(pid)
    }

    fn peek_dir_entry_for_pid(&mut self, pid: usize, fd: i32) -> Result<Option<FsDirEntry>, i32> {
        let (path, cursor) = {
            let Some(table) = self.open_files.get(&pid) else {
                return Err(EBADF);
            };
            let Some(open_file) = table.get(&fd) else {
                return Err(EBADF);
            };
            match open_file {
                OpenFile::Dir(d) => (d.path.clone(), d.cursor),
                _ => return Err(ENOTDIR),
            }
        };

        let entries = self.collect_dir_entries(path.as_str());
        if cursor >= entries.len() {
            return Ok(None);
        }
        Ok(entries.get(cursor).cloned())
    }

    fn advance_dir_cursor_for_pid(&mut self, pid: usize, fd: i32, count: usize) -> Result<(), i32> {
        let table = self.open_files.get_mut(&pid).ok_or(EBADF)?;
        let open_file = table.get_mut(&fd).ok_or(EBADF)?;
        match open_file {
            OpenFile::Dir(d) => {
                d.cursor = d.cursor.saturating_add(count);
                Ok(())
            }
            _ => Err(ENOTDIR),
        }
    }

    fn hash_path(path: &str) -> u64 {
        hash_path_impl(path)
    }

    fn make_regular_stat(path: &str, data_len: usize) -> FsStat {
        let size = data_len as i64;
        FsStat {
            dev: 1,
            ino: Self::hash_path(path),
            mode: S_IFREG | 0o644,
            nlink: 1,
            rdev: 0,
            size,
            blksize: 4096,
            blocks: (size.saturating_add(511) / 512),
        }
    }

    fn make_dir_stat(path: &str) -> FsStat {
        FsStat {
            dev: 1,
            ino: Self::hash_path(path),
            mode: S_IFDIR | 0o755,
            nlink: 2,
            rdev: 0,
            size: 0,
            blksize: 4096,
            blocks: 0,
        }
    }

    fn make_pipe_stat(pipe_id: u64, size: usize) -> FsStat {
        let bytes = size as i64;
        FsStat {
            dev: 2,
            ino: pipe_id,
            mode: S_IFIFO | 0o600,
            nlink: 1,
            rdev: 0,
            size: bytes,
            blksize: 4096,
            blocks: (bytes.saturating_add(511) / 512),
        }
    }

    fn make_tty_stat() -> FsStat {
        FsStat {
            dev: 3,
            ino: 1,
            mode: S_IFCHR | 0o620,
            nlink: 1,
            rdev: 0,
            size: 0,
            blksize: 4096,
            blocks: 0,
        }
    }

    fn stat_path(&self, path: &str) -> Result<FsStat, i32> {
        if self.dir_exists(path) {
            return Ok(Self::make_dir_stat(path));
        }
        let Some(file) = self.files.iter().find(|file| file.path == path) else {
            return Err(ENOENT);
        };
        Ok(Self::make_regular_stat(file.path, file.data.len()))
    }

    fn stat_path_for_pid(&mut self, pid: usize, path: &str) -> Result<FsStat, i32> {
        let resolved = self.resolve_path_for_pid(pid, path)?;
        self.stat_path(resolved.as_str())
    }

    fn stat_fd(&self, pid: usize, fd: i32) -> Result<FsStat, i32> {
        if (0..=2).contains(&fd) {
            return Ok(Self::make_tty_stat());
        }

        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };

        match open_file {
            OpenFile::Static(file) => Ok(Self::make_regular_stat(file.path, file.data.len())),
            OpenFile::Vfs(file) => {
                let size = file
                    .static_data
                    .map(|data| data.len() as u64)
                    .unwrap_or_else(|| file.inode.metadata().map(|m| m.size).unwrap_or(0));
                Ok(FsStat {
                    dev: 1,
                    ino: file.ino,
                    mode: file.mode,
                    nlink: 1,
                    rdev: 0,
                    size: size as i64,
                    blksize: 4096,
                    blocks: ((size as i64).saturating_add(511) / 512),
                })
            }
            OpenFile::Dir(dir) => {
                Ok(FsStat {
                    dev: 1,
                    ino: dir.ino,
                    mode: dir.mode,
                    nlink: 2,
                    rdev: 0,
                    size: 0,
                    blksize: 4096,
                    blocks: 0,
                })
            }
            OpenFile::PipeRead { pipe_id, .. } | OpenFile::PipeWrite { pipe_id, .. } => {
                let size = self
                    .pipes
                    .get(pipe_id)
                    .map(|pipe| pipe.data.len())
                    .unwrap_or(0);
                Ok(Self::make_pipe_stat(*pipe_id, size))
            }
        }
    }

    fn poll_revents_for_pid(&self, pid: usize, fd: i32, events: i16) -> Result<i16, i32> {
        if fd < 0 {
            return Ok(POLLNVAL);
        }

        if (0..=2).contains(&fd) {
            let mut revents = 0i16;
            if (events & POLLIN) != 0 && fd == 0 && crate::drivers::tty::serial_has_input() {
                revents |= POLLIN;
            }
            if (events & POLLOUT) != 0 && (fd == 1 || fd == 2) {
                revents |= POLLOUT;
            }
            return Ok(revents);
        }

        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };

        let mut revents = 0i16;

        match open_file {
            OpenFile::Static(file) => {
                if (events & POLLIN) != 0 {
                    // 文件读到 EOF 也算可读（read 会返回 0）
                    let eof = file.offset >= file.data.len();
                    if !file.data.is_empty() || eof {
                        revents |= POLLIN;
                    }
                }
            }
            OpenFile::Vfs(file) => {
                if (events & POLLIN) != 0 {
                    if let Some(data) = file.static_data {
                        let eof = file.offset >= data.len();
                        if !data.is_empty() || eof {
                            revents |= POLLIN;
                        }
                    } else if let Ok(meta) = file.inode.metadata() {
                        let eof = file.offset >= meta.size as usize;
                        if meta.size > 0 || eof {
                            revents |= POLLIN;
                        }
                    }
                }
            }
            OpenFile::Dir(_) => {
                if (events & POLLIN) != 0 {
                    revents |= POLLIN;
                }
            }
            OpenFile::PipeRead { pipe_id, .. } => {
                if let Some(pipe) = self.pipes.get(pipe_id) {
                    if (events & POLLIN) != 0 && (!pipe.data.is_empty() || pipe.writers == 0) {
                        revents |= POLLIN;
                    }
                    if pipe.writers == 0 {
                        revents |= POLLHUP;
                    }
                } else {
                    revents |= POLLHUP;
                }
            }
            OpenFile::PipeWrite { pipe_id, .. } => {
                if let Some(pipe) = self.pipes.get(pipe_id) {
                    if pipe.readers == 0 {
                        revents |= POLLERR | POLLHUP;
                    } else if (events & POLLOUT) != 0 {
                        revents |= POLLOUT;
                    }
                } else {
                    revents |= POLLERR | POLLHUP;
                }
            }
        }

        Ok(revents)
    }

    fn drop_process_fds(&mut self, pid: usize) {
        if let Some(table) = self.open_files.remove(&pid) {
            for (_fd, file) in table {
                match file {
                    OpenFile::PipeRead { pipe_id, .. } => {
                        if let Some(pipe) = self.pipes.get_mut(&pipe_id) {
                            pipe.readers = pipe.readers.saturating_sub(1);
                            if pipe.readers == 0 && pipe.writers == 0 {
                                self.pipes.remove(&pipe_id);
                            }
                        }
                    }
                    OpenFile::PipeWrite { pipe_id, .. } => {
                        if let Some(pipe) = self.pipes.get_mut(&pipe_id) {
                            pipe.writers = pipe.writers.saturating_sub(1);
                            if pipe.readers == 0 && pipe.writers == 0 {
                                self.pipes.remove(&pipe_id);
                            }
                        }
                    }
                    OpenFile::Dir(_) => {}
                    OpenFile::Static(_) => {}
                    OpenFile::Vfs(_) => {}
                }
            }
        }
        self.next_fd.remove(&pid);
        self.cwd.remove(&pid);
    }
}

static FS_STATE: Mutex<FsState> = Mutex::new(FsState::new());

pub fn init() {
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("\x1b[90m[diag]\x1b[0m[fs] init minimal static backend");
    }
    vfs::mount_root(Arc::new(vfs::staticfs::StaticFileSystem::new()));
}

pub fn register_static_file(path: &'static str, data: &'static [u8]) -> Result<(), i32> {
    FS_STATE.lock().register_static_file(path, data)
}

pub fn read_static_file(path: &str) -> Option<&'static [u8]> {
    FS_STATE.lock().read_static_file(path)
}

pub fn open_for_pid(pid: usize, path: &str, flags: u32, mode: u16) -> Result<i32, i32> {
    let resolved = {
        let mut state = FS_STATE.lock();
        state.resolve_path_for_pid(pid, path)?
    };
    let inode = vfs::lookup_path(resolved.as_str()).map_err(|e| e.errno())?;
    let meta = inode.metadata().map_err(|e| e.errno())?;
    let static_data = crate::fs::backend_read_static_file(resolved.as_str());
    let vfs_hint = Some(VfsOpenHint {
        inode,
        meta,
        static_data,
    });
    FS_STATE
        .lock()
        .open_for_pid(pid, resolved.as_str(), flags, mode, vfs_hint)
}

pub fn read_for_pid(pid: usize, fd: i32, out: &mut [u8]) -> Result<usize, i32> {
    FS_STATE.lock().read_for_pid(pid, fd, out)
}

pub fn write_for_pid(pid: usize, fd: i32, data: &[u8]) -> Result<usize, i32> {
    FS_STATE.lock().write_for_pid(pid, fd, data)
}

pub fn pipe2_for_pid(pid: usize, flags: u32) -> Result<(i32, i32), i32> {
    FS_STATE.lock().pipe2_for_pid(pid, flags)
}

pub fn close_for_pid(pid: usize, fd: i32) -> Result<(), i32> {
    FS_STATE.lock().close_for_pid(pid, fd)
}

pub fn read_at_for_pid(pid: usize, fd: i32, offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    FS_STATE.lock().read_at_for_pid(pid, fd, offset, out)
}

pub fn mmap_file_for_pid(pid: usize, fd: i32) -> Result<&'static [u8], i32> {
    FS_STATE.lock().mmap_file_for_pid(pid, fd)
}

pub fn fd_is_nonblocking_for_pid(pid: usize, fd: i32) -> Result<bool, i32> {
    FS_STATE.lock().fd_is_nonblocking_for_pid(pid, fd)
}

pub fn lseek_for_pid(pid: usize, fd: i32, offset: i64, whence: u32) -> Result<usize, i32> {
    FS_STATE.lock().lseek_for_pid(pid, fd, offset, whence)
}

pub fn dup_for_pid(pid: usize, oldfd: i32, min_newfd: i32, cloexec: bool) -> Result<i32, i32> {
    FS_STATE.lock().dup_for_pid(pid, oldfd, min_newfd, cloexec)
}

pub fn dup2_for_pid(pid: usize, oldfd: i32, newfd: i32, cloexec: bool) -> Result<i32, i32> {
    FS_STATE.lock().dup2_for_pid(pid, oldfd, newfd, cloexec)
}

pub fn fcntl_getfl_for_pid(pid: usize, fd: i32) -> Result<u32, i32> {
    FS_STATE.lock().fcntl_getfl_for_pid(pid, fd)
}

pub fn fcntl_setfl_for_pid(pid: usize, fd: i32, flags: u32) -> Result<(), i32> {
    FS_STATE.lock().fcntl_setfl_for_pid(pid, fd, flags)
}

pub fn fcntl_getfd_for_pid(pid: usize, fd: i32) -> Result<u32, i32> {
    FS_STATE.lock().fcntl_getfd_for_pid(pid, fd)
}

pub fn fcntl_setfd_for_pid(pid: usize, fd: i32, cloexec: bool) -> Result<(), i32> {
    FS_STATE.lock().fcntl_setfd_for_pid(pid, fd, cloexec)
}

pub fn chdir_for_pid(pid: usize, path: &str) -> Result<(), i32> {
    let resolved = {
        let mut state = FS_STATE.lock();
        state.resolve_path_for_pid(pid, path)?
    };
    let inode = vfs::lookup_path(resolved.as_str()).map_err(|e| e.errno())?;
    let meta = inode.metadata().map_err(|e| e.errno())?;
    if meta.file_type != vfs::FileType::Directory {
        return Err(ENOTDIR);
    }
    let mut state = FS_STATE.lock();
    let cwd = state.ensure_cwd_mut(pid);
    *cwd = resolved;
    Ok(())
}

pub fn getcwd_for_pid(pid: usize) -> String {
    FS_STATE.lock().getcwd_for_pid(pid)
}

pub fn peek_dir_entry_for_pid(pid: usize, fd: i32) -> Result<Option<FsDirEntry>, i32> {
    let (cursor, dir_inode) = {
        let state = FS_STATE.lock();
        let table = state.open_files.get(&pid).ok_or(EBADF)?;
        let open_file = table.get(&fd).ok_or(EBADF)?;
        match open_file {
            OpenFile::Dir(dir) => (dir.cursor, dir.inode.clone()),
            _ => return Err(ENOTDIR),
        }
    };

    let Some(inode) = dir_inode else {
        return Err(ENOTDIR);
    };

    let entries = inode.readdir().map_err(|e| e.errno())?;
    if cursor >= entries.len() {
        return Ok(None);
    }
    let entry = &entries[cursor];
    let file_type = match entry.file_type {
        vfs::FileType::Directory => DT_DIR,
        vfs::FileType::Regular => DT_REG,
        _ => DT_UNKNOWN,
    };
    Ok(Some(FsDirEntry {
        ino: entry.ino,
        file_type,
        name: entry.name.clone(),
    }))
}

pub fn advance_dir_cursor_for_pid(pid: usize, fd: i32, count: usize) -> Result<(), i32> {
    FS_STATE.lock().advance_dir_cursor_for_pid(pid, fd, count)
}

pub fn stat_path(path: &str) -> Result<FsStat, i32> {
    let inode = vfs::lookup_path(path).map_err(|e| e.errno())?;
    let meta = inode.metadata().map_err(|e| e.errno())?;
    Ok(FsStat {
        dev: 1,
        ino: meta.ino,
        mode: meta.mode,
        nlink: meta.nlink as u64,
        rdev: 0,
        size: meta.size as i64,
        blksize: 4096,
        blocks: ((meta.size as i64).saturating_add(511) / 512),
    })
}

pub fn stat_path_for_pid(pid: usize, path: &str) -> Result<FsStat, i32> {
    let resolved = {
        let mut state = FS_STATE.lock();
        state.resolve_path_for_pid(pid, path)?
    };
    let inode = vfs::lookup_path(resolved.as_str()).map_err(|e| e.errno())?;
    let meta = inode.metadata().map_err(|e| e.errno())?;
    Ok(FsStat {
        dev: 1,
        ino: meta.ino,
        mode: meta.mode,
        nlink: meta.nlink as u64,
        rdev: 0,
        size: meta.size as i64,
        blksize: 4096,
        blocks: ((meta.size as i64).saturating_add(511) / 512),
    })
}

pub fn stat_fd(pid: usize, fd: i32) -> Result<FsStat, i32> {
    FS_STATE.lock().stat_fd(pid, fd)
}

pub fn poll_revents_for_pid(pid: usize, fd: i32, events: i16) -> Result<i16, i32> {
    FS_STATE.lock().poll_revents_for_pid(pid, fd, events)
}

pub fn drop_process_fds(pid: usize) {
    FS_STATE.lock().drop_process_fds(pid);
}

pub(crate) fn backend_read_static_file(path: &str) -> Option<&'static [u8]> {
    FS_STATE.lock().read_static_file(path)
}

pub(crate) fn backend_dir_exists(path: &str) -> bool {
    FS_STATE.lock().dir_exists(path)
}

pub(crate) fn backend_collect_dir_entries(path: &str) -> Vec<FsDirEntry> {
    FS_STATE.lock().collect_dir_entries(path)
}

pub(crate) fn backend_hash_path(path: &str) -> u64 {
    hash_path_impl(path)
}

pub(crate) const BACKEND_DT_DIR: u8 = DT_DIR;
pub(crate) const BACKEND_DT_REG: u8 = DT_REG;
