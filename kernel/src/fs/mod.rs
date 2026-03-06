//! File descriptor table and syscall-facing FS glue.
//!
//! Runtime file semantics are provided by VFS in `fs/vfs`.

pub mod backing;
pub mod runtime;
pub mod vfs;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::sync::Mutex;
use crate::syscall::{E2BIG, EAGAIN, EBADF, EINVAL, EISDIR, ENOENT, ENOTDIR, EPIPE, ESPIPE};

const FIRST_USER_FD: i32 = 3;
const O_CLOEXEC: u32 = 0o2000000;
const O_NONBLOCK: u32 = 0o4000;
const O_DIRECTORY: u32 = 0o200000;
const O_ACCMODE: u32 = 0o3;
const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;

const S_IFIFO: u32 = 0o010000;
const S_IFCHR: u32 = 0o020000;

const POLLIN: i16 = 0x0001;
const POLLOUT: i16 = 0x0004;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

const DT_UNKNOWN: u8 = 0;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;

#[derive(Clone)]
struct DirOpenFile {
    cursor: usize,
    cloexec: bool,
    inode: Arc<dyn vfs::Inode>,
    ino: u64,
    mode: u32,
}

#[derive(Clone)]
struct VfsOpenFile {
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

struct MmapBacking {
    inode: Arc<dyn vfs::Inode>,
    static_data: Option<&'static [u8]>,
    refs: usize,
}

struct FsState {
    open_files: BTreeMap<usize, BTreeMap<i32, OpenFile>>,
    next_fd: BTreeMap<usize, i32>,
    cwd: BTreeMap<usize, String>,
    next_pipe_id: u64,
    pipes: BTreeMap<u64, PipeState>,
    next_mmap_backing_id: u64,
    mmap_backings: BTreeMap<u64, MmapBacking>,
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

#[derive(Clone)]
pub struct FsDirEntry {
    pub ino: u64,
    pub file_type: u8,
    pub name: String,
}

impl FsState {
    const fn new() -> Self {
        Self {
            open_files: BTreeMap::new(),
            next_fd: BTreeMap::new(),
            cwd: BTreeMap::new(),
            next_pipe_id: 1,
            pipes: BTreeMap::new(),
            next_mmap_backing_id: 1,
            mmap_backings: BTreeMap::new(),
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

    fn open_for_pid(
        &mut self,
        pid: usize,
        path: &str,
        flags: u32,
        _mode: u16,
        hint: VfsOpenHint,
    ) -> Result<i32, i32> {
        let _resolved = self.resolve_path_for_pid(pid, path)?;
        let cloexec = (flags & O_CLOEXEC) != 0;
        let wants_dir = (flags & O_DIRECTORY) != 0;
        let accmode = flags & O_ACCMODE;
        if accmode != O_RDONLY && accmode != O_WRONLY {
            return Err(EINVAL);
        }

        let open_file = match hint.meta.file_type {
            vfs::FileType::Directory => {
                if accmode != O_RDONLY {
                    return Err(EISDIR);
                }
                OpenFile::Dir(DirOpenFile {
                    cursor: 0,
                    cloexec,
                    inode: hint.inode,
                    ino: hint.meta.ino,
                    mode: hint.meta.mode,
                })
            }
            vfs::FileType::Regular => {
                if wants_dir {
                    return Err(ENOTDIR);
                }
                OpenFile::Vfs(VfsOpenFile {
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
            OpenFile::Dir(_) | OpenFile::Vfs(_) => {}
        }
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

    fn fd_is_nonblocking_for_pid(&self, pid: usize, fd: i32) -> Result<bool, i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };

        match open_file {
            OpenFile::Vfs(_) | OpenFile::Dir(_) => Ok(false),
            OpenFile::PipeRead { nonblocking, .. } => Ok(*nonblocking),
            OpenFile::PipeWrite { nonblocking, .. } => Ok(*nonblocking),
        }
    }

    fn lseek_for_pid(
        &mut self,
        pid: usize,
        fd: i32,
        offset: i64,
        whence: u32,
    ) -> Result<usize, i32> {
        let Some(table) = self.open_files.get_mut(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get_mut(&fd) else {
            return Err(EBADF);
        };

        match open_file {
            OpenFile::Vfs(file) => {
                let size = file
                    .static_data
                    .map(|data| data.len() as i64)
                    .unwrap_or_else(|| file.inode.metadata().map(|m| m.size as i64).unwrap_or(0));
                let base = match whence {
                    0 => 0i64,
                    1 => file.offset as i64,
                    2 => size,
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
            OpenFile::Vfs(_) | OpenFile::Dir(_) => {}
        }
    }

    fn dup_for_pid(
        &mut self,
        pid: usize,
        oldfd: i32,
        min_newfd: i32,
        cloexec: bool,
    ) -> Result<i32, i32> {
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

    fn dup2_for_pid(
        &mut self,
        pid: usize,
        oldfd: i32,
        newfd: i32,
        cloexec: bool,
    ) -> Result<i32, i32> {
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
            OpenFile::Vfs(_) | OpenFile::Dir(_) => O_RDONLY,
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
            OpenFile::Vfs(_) | OpenFile::Dir(_) => {}
        }
        Ok(())
    }

    fn fcntl_getfd_for_pid(&self, pid: usize, fd: i32) -> Result<u32, i32> {
        let table = self.open_files.get(&pid).ok_or(EBADF)?;
        let open_file = table.get(&fd).ok_or(EBADF)?;
        let cloexec = match open_file {
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

    fn peek_dir_entry_for_pid(&self, pid: usize, fd: i32) -> Result<Option<FsDirEntry>, i32> {
        let (cursor, inode) = {
            let table = self.open_files.get(&pid).ok_or(EBADF)?;
            let open_file = table.get(&fd).ok_or(EBADF)?;
            match open_file {
                OpenFile::Dir(d) => (d.cursor, d.inode.clone()),
                _ => return Err(ENOTDIR),
            }
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

    fn stat_from_meta(meta: vfs::Metadata) -> FsStat {
        let size = meta.size as i64;
        FsStat {
            dev: 1,
            ino: meta.ino,
            mode: meta.mode,
            nlink: meta.nlink as u64,
            rdev: 0,
            size,
            blksize: 4096,
            blocks: size.saturating_add(511) / 512,
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
            blocks: bytes.saturating_add(511) / 512,
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
            OpenFile::Vfs(file) => {
                if let Some(data) = file.static_data {
                    let size = data.len() as i64;
                    Ok(FsStat {
                        dev: 1,
                        ino: file.ino,
                        mode: file.mode,
                        nlink: 1,
                        rdev: 0,
                        size,
                        blksize: 4096,
                        blocks: size.saturating_add(511) / 512,
                    })
                } else {
                    let meta = file.inode.metadata().map_err(|e| e.errno())?;
                    Ok(Self::stat_from_meta(meta))
                }
            }
            OpenFile::Dir(dir) => Ok(FsStat {
                dev: 1,
                ino: dir.ino,
                mode: dir.mode,
                nlink: 2,
                rdev: 0,
                size: 0,
                blksize: 4096,
                blocks: 0,
            }),
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
                    OpenFile::Dir(_) | OpenFile::Vfs(_) => {}
                }
            }
        }

        self.next_fd.remove(&pid);
        self.cwd.remove(&pid);
    }

    fn create_mmap_backing_for_pid(&mut self, pid: usize, fd: i32) -> Result<u64, i32> {
        let table = self.open_files.get(&pid).ok_or(EBADF)?;
        let open_file = table.get(&fd).ok_or(EBADF)?;
        let (inode, static_data) = match open_file {
            OpenFile::Vfs(file) => (file.inode.clone(), file.static_data),
            OpenFile::Dir(_) => return Err(EISDIR),
            _ => return Err(EBADF),
        };

        let backing_id = self.next_mmap_backing_id;
        self.next_mmap_backing_id = self.next_mmap_backing_id.saturating_add(1);
        self.mmap_backings.insert(
            backing_id,
            MmapBacking {
                inode,
                static_data,
                refs: 1,
            },
        );
        Ok(backing_id)
    }

    fn retain_mmap_backing(&mut self, backing_id: u64) -> Result<(), i32> {
        let Some(backing) = self.mmap_backings.get_mut(&backing_id) else {
            return Err(EBADF);
        };
        backing.refs = backing.refs.saturating_add(1);
        Ok(())
    }

    fn release_mmap_backing(&mut self, backing_id: u64) {
        let mut should_remove = false;
        if let Some(backing) = self.mmap_backings.get_mut(&backing_id) {
            if backing.refs > 1 {
                backing.refs -= 1;
            } else {
                should_remove = true;
            }
        }
        if should_remove {
            self.mmap_backings.remove(&backing_id);
        }
    }

    fn copy_mmap_page(
        &self,
        backing_id: u64,
        file_offset: usize,
        out: &mut [u8],
    ) -> Result<usize, i32> {
        let backing = self.mmap_backings.get(&backing_id).ok_or(EBADF)?;
        if let Some(data) = backing.static_data {
            if file_offset >= data.len() {
                return Ok(0);
            }
            let n = out.len().min(data.len() - file_offset);
            out[..n].copy_from_slice(&data[file_offset..file_offset + n]);
            return Ok(n);
        }
        backing
            .inode
            .read_at(file_offset, out)
            .map_err(|e| e.errno())
    }

    fn read_all_for_pid(&mut self, pid: usize, path: &str) -> Result<Vec<u8>, i32> {
        let resolved = self.resolve_path_for_pid(pid, path)?;
        let inode = vfs::lookup_path(resolved.as_str()).map_err(|e| e.errno())?;
        let meta = inode.metadata().map_err(|e| e.errno())?;
        if meta.file_type != vfs::FileType::Regular {
            return Err(ENOENT);
        }

        if let Some(data) = vfs::initramfs::read_file(resolved.as_str()) {
            return Ok(data.to_vec());
        }

        let size = usize::try_from(meta.size).map_err(|_| E2BIG)?;
        if size == 0 {
            return Ok(Vec::new());
        }

        let mut out = vec![0u8; size];
        let mut offset = 0usize;
        while offset < out.len() {
            let n = inode
                .read_at(offset, &mut out[offset..])
                .map_err(|e| e.errno())?;
            if n == 0 {
                break;
            }
            offset = offset.saturating_add(n);
        }
        out.truncate(offset);
        Ok(out)
    }
}

static FS_STATE: Mutex<FsState> = Mutex::new(FsState::new());

#[derive(Debug, Clone, Copy)]
pub struct FsInitReport {
    pub initramfs_present: bool,
    pub rootfs: &'static str,
}

pub fn init(initramfs: Option<(u64, u64)>) {
    let _ = init_runtime(initramfs);
}

pub fn init_runtime(initramfs: Option<(u64, u64)>) -> FsInitReport {
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("\x1b[90m[diag]\x1b[0m[fs] init vfs-backed fd runtime");
    }
    let initramfs_present = initramfs.is_some();
    if let Some((phys, size)) = initramfs {
        if let Err(err) = vfs::initramfs::init_from_phys(phys, size) {
            let _ = vfs::initramfs::init_from_phys(0, 0);
            crate::warn!("[fs] initramfs parse failed: {:?}", err);
        }
    } else {
        let _ = vfs::initramfs::init_from_phys(0, 0);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!("\x1b[90m[diag]\x1b[0m[fs] initramfs not provided");
        }
    }
    vfs::mount_root(Arc::new(vfs::initramfs::InitramfsFileSystem::new()));

    FsInitReport {
        initramfs_present,
        rootfs: "initramfs",
    }
}

pub fn open_for_pid(pid: usize, path: &str, flags: u32, mode: u16) -> Result<i32, i32> {
    let resolved = {
        let mut state = FS_STATE.lock();
        state.resolve_path_for_pid(pid, path)?
    };

    let inode = vfs::lookup_path(resolved.as_str()).map_err(|e| e.errno())?;
    let meta = inode.metadata().map_err(|e| e.errno())?;
    let static_data = vfs::initramfs::read_file(resolved.as_str());

    let hint = VfsOpenHint {
        inode,
        meta,
        static_data,
    };

    FS_STATE
        .lock()
        .open_for_pid(pid, resolved.as_str(), flags, mode, hint)
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
    FS_STATE.lock().chdir_for_pid(pid, path)
}

pub fn getcwd_for_pid(pid: usize) -> String {
    FS_STATE.lock().getcwd_for_pid(pid)
}

pub fn peek_dir_entry_for_pid(pid: usize, fd: i32) -> Result<Option<FsDirEntry>, i32> {
    FS_STATE.lock().peek_dir_entry_for_pid(pid, fd)
}

pub fn advance_dir_cursor_for_pid(pid: usize, fd: i32, count: usize) -> Result<(), i32> {
    FS_STATE.lock().advance_dir_cursor_for_pid(pid, fd, count)
}

pub fn stat_path(path: &str) -> Result<FsStat, i32> {
    let inode = vfs::lookup_path(path).map_err(|e| e.errno())?;
    let meta = inode.metadata().map_err(|e| e.errno())?;
    Ok(FsState::stat_from_meta(meta))
}

pub fn stat_path_for_pid(pid: usize, path: &str) -> Result<FsStat, i32> {
    let resolved = {
        let mut state = FS_STATE.lock();
        state.resolve_path_for_pid(pid, path)?
    };
    stat_path(resolved.as_str())
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

pub fn read_all_for_pid(pid: usize, path: &str) -> Result<Vec<u8>, i32> {
    FS_STATE.lock().read_all_for_pid(pid, path)
}

pub fn mmap_create_backing_for_pid(pid: usize, fd: i32) -> Result<u64, i32> {
    FS_STATE.lock().create_mmap_backing_for_pid(pid, fd)
}

pub fn mmap_retain_backing(backing_id: u64) -> Result<(), i32> {
    FS_STATE.lock().retain_mmap_backing(backing_id)
}

pub fn mmap_release_backing(backing_id: u64) {
    FS_STATE.lock().release_mmap_backing(backing_id);
}

pub fn mmap_copy_page(backing_id: u64, file_offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    FS_STATE.lock().copy_mmap_page(backing_id, file_offset, out)
}
