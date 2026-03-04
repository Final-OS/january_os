//! Minimal in-kernel file backend for v0.2 syscall bring-up.
//!
//! This module intentionally keeps scope small:
//! - static read-only files (registered at runtime)
//! - per-process fd table for `open/read/close`
//! - lookup API for `execve` image provider

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::sync::Mutex;
use crate::syscall::{EAGAIN, EBADF, EINVAL, ENOENT, EPIPE};

const FIRST_USER_FD: i32 = 3;
const O_CLOEXEC: u32 = 0o2000000;
const O_NONBLOCK: u32 = 0o4000;

#[derive(Clone, Copy)]
struct StaticFile {
    path: &'static str,
    data: &'static [u8],
}

#[derive(Clone, Copy)]
struct StaticOpenFile {
    path: &'static str,
    data: &'static [u8],
    offset: usize,
}

#[derive(Clone, Copy)]
enum OpenFile {
    Static(StaticOpenFile),
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
const S_IFREG: u32 = 0o100000;

const POLLIN: i16 = 0x0001;
const POLLOUT: i16 = 0x0004;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

impl FsState {
    const fn new() -> Self {
        Self {
            files: Vec::new(),
            open_files: BTreeMap::new(),
            next_fd: BTreeMap::new(),
            next_pipe_id: 1,
            pipes: BTreeMap::new(),
        }
    }

    fn alloc_fd(&mut self, pid: usize) -> i32 {
        let next_fd = self.next_fd.entry(pid).or_insert(FIRST_USER_FD);
        let fd = *next_fd;
        *next_fd = next_fd.saturating_add(1);
        fd
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
        _flags: u32,
        _mode: u16,
    ) -> Result<i32, i32> {
        let Some(file) = self.files.iter().find(|file| file.path == path).copied() else {
            return Err(ENOENT);
        };

        let fd = self.alloc_fd(pid);

        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(
            fd,
            OpenFile::Static(StaticOpenFile {
                path: file.path,
                data: file.data,
                offset: 0,
            }),
        );
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
        let Some(open_file) = table.get(&fd).copied() else {
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
            OpenFile::PipeWrite { .. } => Err(EBADF),
        }
    }

    fn write_for_pid(&mut self, pid: usize, fd: i32, data: &[u8]) -> Result<usize, i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd).copied() else {
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
            OpenFile::Static(_) => {}
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
        let OpenFile::Static(open_file) = open_file else {
            return Err(EBADF);
        };

        if out.is_empty() {
            return Ok(0);
        }

        if offset >= open_file.data.len() {
            return Ok(0);
        }

        let remain = open_file.data.len().saturating_sub(offset);
        let read_len = remain.min(out.len());
        out[..read_len].copy_from_slice(&open_file.data[offset..offset.saturating_add(read_len)]);
        Ok(read_len)
    }

    fn mmap_file_for_pid(&self, pid: usize, fd: i32) -> Result<&'static [u8], i32> {
        let Some(table) = self.open_files.get(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get(&fd) else {
            return Err(EBADF);
        };
        let OpenFile::Static(open_file) = open_file else {
            return Err(EBADF);
        };
        Ok(open_file.data)
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
            OpenFile::PipeRead { nonblocking, .. } => Ok(*nonblocking),
            OpenFile::PipeWrite { nonblocking, .. } => Ok(*nonblocking),
        }
    }

    fn hash_path(path: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in path.as_bytes().iter().copied() {
            h ^= b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
        h
    }

    fn make_regular_stat(path: &'static str, data_len: usize) -> FsStat {
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
        let Some(file) = self.files.iter().find(|file| file.path == path) else {
            return Err(ENOENT);
        };
        Ok(Self::make_regular_stat(file.path, file.data.len()))
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
                    OpenFile::Static(_) => {}
                }
            }
        }
        self.next_fd.remove(&pid);
    }
}

static FS_STATE: Mutex<FsState> = Mutex::new(FsState::new());

pub fn init() {
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!("\x1b[90m[diag]\x1b[0m[fs] init minimal static backend");
    }
}

pub fn register_static_file(path: &'static str, data: &'static [u8]) -> Result<(), i32> {
    FS_STATE.lock().register_static_file(path, data)
}

pub fn read_static_file(path: &str) -> Option<&'static [u8]> {
    FS_STATE.lock().read_static_file(path)
}

pub fn open_for_pid(pid: usize, path: &str, flags: u32, mode: u16) -> Result<i32, i32> {
    FS_STATE.lock().open_for_pid(pid, path, flags, mode)
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

pub fn stat_path(path: &str) -> Result<FsStat, i32> {
    FS_STATE.lock().stat_path(path)
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
