//! Minimal in-kernel file backend for v0.2 syscall bring-up.
//!
//! This module intentionally keeps scope small:
//! - static read-only files (registered at runtime)
//! - per-process fd table for `open/read/close`
//! - lookup API for `execve` image provider

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::sync::Mutex;
use crate::syscall::{EBADF, EINVAL, ENOENT};

const FIRST_USER_FD: i32 = 3;

#[derive(Clone, Copy)]
struct StaticFile {
    path: &'static str,
    data: &'static [u8],
}

#[derive(Clone, Copy)]
struct OpenFile {
    data: &'static [u8],
    offset: usize,
}

struct FsState {
    files: Vec<StaticFile>,
    open_files: BTreeMap<usize, BTreeMap<i32, OpenFile>>,
    next_fd: BTreeMap<usize, i32>,
}

impl FsState {
    const fn new() -> Self {
        Self {
            files: Vec::new(),
            open_files: BTreeMap::new(),
            next_fd: BTreeMap::new(),
        }
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

        let next_fd = self.next_fd.entry(pid).or_insert(FIRST_USER_FD);
        let fd = *next_fd;
        *next_fd = next_fd.saturating_add(1);

        let table = self.open_files.entry(pid).or_insert_with(BTreeMap::new);
        table.insert(fd, OpenFile { data, offset: 0 });
        Ok(fd)
    }

    fn read_for_pid(&mut self, pid: usize, fd: i32, out: &mut [u8]) -> Result<usize, i32> {
        let Some(table) = self.open_files.get_mut(&pid) else {
            return Err(EBADF);
        };
        let Some(open_file) = table.get_mut(&fd) else {
            return Err(EBADF);
        };

        if out.is_empty() {
            return Ok(0);
        }

        let remain = open_file.data.len().saturating_sub(open_file.offset);
        let read_len = remain.min(out.len());
        let end = open_file.offset.saturating_add(read_len);
        out[..read_len].copy_from_slice(&open_file.data[open_file.offset..end]);
        open_file.offset = end;
        Ok(read_len)
    }

    fn close_for_pid(&mut self, pid: usize, fd: i32) -> Result<(), i32> {
        let Some(table) = self.open_files.get_mut(&pid) else {
            return Err(EBADF);
        };
        if table.remove(&fd).is_none() {
            return Err(EBADF);
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
        Ok(open_file.data)
    }

    fn drop_process_fds(&mut self, pid: usize) {
        self.open_files.remove(&pid);
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
