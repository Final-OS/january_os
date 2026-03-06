//! File descriptor table and syscall-facing FS glue.
//!
//! Runtime file semantics are provided by `fs::runtime`, `fs::fd`, `fs::pipe`,
//! `fs::backing` and `fs::vfs`.

pub mod api;
pub mod backing;
pub mod diag;
pub mod fd;
pub mod pipe;
pub mod runtime;
pub mod syscall;
pub mod vfs;

pub use api::{DirEntry, FileType, FsError, Metadata, SeekWhence};
pub use runtime::manager::{
    chdir_for_pid, close_for_pid, drop_process_fds, dump_state, dup2_for_pid, dup_for_pid,
    fcntl_getfd_for_pid, fcntl_getfl_for_pid, fcntl_setfd_for_pid, fcntl_setfl_for_pid,
    fd_is_nonblocking_for_pid, getcwd_for_pid, init, init_core, init_early, init_late,
    init_runtime, lseek_for_pid, mmap_copy_page, mmap_create_backing_for_pid,
    mmap_release_backing, mmap_retain_backing, open_for_pid, peek_dir_entry_for_pid,
    pipe2_for_pid, poll_revents_for_pid, read_all_for_pid, read_at_for_pid, read_for_pid,
    stat_fd, stat_path, stat_path_for_pid, stats, write_for_pid, wake_stdin_waiters_if_ready,
    advance_dir_cursor_for_pid, COMPONENT, FsDirEntry, FsInitReport, FsStat,
};
pub use vfs::{lookup_path, File, FileSystem, Inode, MountEntry};
