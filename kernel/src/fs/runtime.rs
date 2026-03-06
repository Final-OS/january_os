use alloc::string::String;
use alloc::vec::Vec;

use super::{FsDirEntry, FsStat};

pub fn init(initramfs: Option<(u64, u64)>) -> super::FsInitReport {
    super::init_runtime(initramfs)
}

pub fn open_for_pid(pid: usize, path: &str, flags: u32, mode: u16) -> Result<i32, i32> {
    super::open_for_pid(pid, path, flags, mode)
}

pub fn read_for_pid(pid: usize, fd: i32, out: &mut [u8]) -> Result<usize, i32> {
    super::read_for_pid(pid, fd, out)
}

pub fn write_for_pid(pid: usize, fd: i32, data: &[u8]) -> Result<usize, i32> {
    super::write_for_pid(pid, fd, data)
}

pub fn pipe2_for_pid(pid: usize, flags: u32) -> Result<(i32, i32), i32> {
    super::pipe2_for_pid(pid, flags)
}

pub fn close_for_pid(pid: usize, fd: i32) -> Result<(), i32> {
    super::close_for_pid(pid, fd)
}

pub fn read_at_for_pid(pid: usize, fd: i32, offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    super::read_at_for_pid(pid, fd, offset, out)
}

pub fn fd_is_nonblocking_for_pid(pid: usize, fd: i32) -> Result<bool, i32> {
    super::fd_is_nonblocking_for_pid(pid, fd)
}

pub fn lseek_for_pid(pid: usize, fd: i32, offset: i64, whence: u32) -> Result<usize, i32> {
    super::lseek_for_pid(pid, fd, offset, whence)
}

pub fn dup_for_pid(pid: usize, oldfd: i32, min_newfd: i32, cloexec: bool) -> Result<i32, i32> {
    super::dup_for_pid(pid, oldfd, min_newfd, cloexec)
}

pub fn dup2_for_pid(pid: usize, oldfd: i32, newfd: i32, cloexec: bool) -> Result<i32, i32> {
    super::dup2_for_pid(pid, oldfd, newfd, cloexec)
}

pub fn fcntl_getfl_for_pid(pid: usize, fd: i32) -> Result<u32, i32> {
    super::fcntl_getfl_for_pid(pid, fd)
}

pub fn fcntl_setfl_for_pid(pid: usize, fd: i32, flags: u32) -> Result<(), i32> {
    super::fcntl_setfl_for_pid(pid, fd, flags)
}

pub fn fcntl_getfd_for_pid(pid: usize, fd: i32) -> Result<u32, i32> {
    super::fcntl_getfd_for_pid(pid, fd)
}

pub fn fcntl_setfd_for_pid(pid: usize, fd: i32, cloexec: bool) -> Result<(), i32> {
    super::fcntl_setfd_for_pid(pid, fd, cloexec)
}

pub fn chdir_for_pid(pid: usize, path: &str) -> Result<(), i32> {
    super::chdir_for_pid(pid, path)
}

pub fn getcwd_for_pid(pid: usize) -> String {
    super::getcwd_for_pid(pid)
}

pub fn peek_dir_entry_for_pid(pid: usize, fd: i32) -> Result<Option<FsDirEntry>, i32> {
    super::peek_dir_entry_for_pid(pid, fd)
}

pub fn advance_dir_cursor_for_pid(pid: usize, fd: i32, count: usize) -> Result<(), i32> {
    super::advance_dir_cursor_for_pid(pid, fd, count)
}

pub fn stat_path(path: &str) -> Result<FsStat, i32> {
    super::stat_path(path)
}

pub fn stat_path_for_pid(pid: usize, path: &str) -> Result<FsStat, i32> {
    super::stat_path_for_pid(pid, path)
}

pub fn stat_fd(pid: usize, fd: i32) -> Result<FsStat, i32> {
    super::stat_fd(pid, fd)
}

pub fn poll_revents_for_pid(pid: usize, fd: i32, events: i16) -> Result<i16, i32> {
    super::poll_revents_for_pid(pid, fd, events)
}

pub fn drop_process_fds(pid: usize) {
    super::drop_process_fds(pid)
}

pub fn read_all_for_pid(pid: usize, path: &str) -> Result<Vec<u8>, i32> {
    super::read_all_for_pid(pid, path)
}
