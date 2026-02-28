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
use crate::syscall::{EBADF, EINVAL, ENOENT, EPIPE};

const FIRST_USER_FD: i32 = 3;

#[derive(Clone, Copy)]
struct StaticFile {
    path: &'static str,
    data: &'static [u8],
}

#[derive(Clone, Copy)]
struct StaticOpenFile {
    data: &'static [u8],
    offset: usize,
}

#[derive(Clone, Copy)]
enum OpenFile {
    Static(StaticOpenFile),
    PipeRead { pipe_id: u64 },
    PipeWrite { pipe_id: u64 },
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
        let Some(data) = self.read_static_file(path) else {
            return Err(ENOENT);
        };

        let fd = self.alloc_fd(pid);

        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(
            fd,
            OpenFile::Static(StaticOpenFile {
                data,
                offset: 0,
            }),
        );
        Ok(fd)
    }

    fn pipe2_for_pid(&mut self, pid: usize, flags: u32) -> Result<(i32, i32), i32> {
        // 当前最小实现不支持额外 flags（如 O_NONBLOCK/O_CLOEXEC）。
        if flags != 0 {
            return Err(EINVAL);
        }

        let pipe_id = self.next_pipe_id;
        self.next_pipe_id = self.next_pipe_id.saturating_add(1);

        let read_fd = self.alloc_fd(pid);
        let write_fd = self.alloc_fd(pid);

        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(read_fd, OpenFile::PipeRead { pipe_id });
        table.insert(write_fd, OpenFile::PipeWrite { pipe_id });
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
            OpenFile::PipeRead { pipe_id } => {
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

                if read_len == 0 && pipe.writers == 0 {
                    Ok(0)
                } else {
                    Ok(read_len)
                }
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
            OpenFile::PipeWrite { pipe_id } => {
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
            OpenFile::PipeRead { pipe_id } => {
                if let Some(pipe) = self.pipes.get_mut(&pipe_id) {
                    pipe.readers = pipe.readers.saturating_sub(1);
                    if pipe.readers == 0 && pipe.writers == 0 {
                        self.pipes.remove(&pipe_id);
                    }
                }
            }
            OpenFile::PipeWrite { pipe_id } => {
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

    fn drop_process_fds(&mut self, pid: usize) {
        if let Some(table) = self.open_files.remove(&pid) {
            for (_fd, file) in table {
                match file {
                    OpenFile::PipeRead { pipe_id } => {
                        if let Some(pipe) = self.pipes.get_mut(&pipe_id) {
                            pipe.readers = pipe.readers.saturating_sub(1);
                            if pipe.readers == 0 && pipe.writers == 0 {
                                self.pipes.remove(&pipe_id);
                            }
                        }
                    }
                    OpenFile::PipeWrite { pipe_id } => {
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
    crate::kprintln!("[diag][fs] init minimal static backend");
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

pub fn drop_process_fds(pid: usize) {
    FS_STATE.lock().drop_process_fds(pid);
}
