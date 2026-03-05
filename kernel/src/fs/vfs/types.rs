use alloc::string::String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    CharDevice,
    Fifo,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub ino: u64,
    pub file_type: FileType,
    pub mode: u32,
    pub size: u64,
    pub nlink: u32,
}

impl Metadata {
    pub const fn empty() -> Self {
        Self {
            ino: 0,
            file_type: FileType::Unknown,
            mode: 0,
            size: 0,
            nlink: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub ino: u64,
    pub file_type: FileType,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekWhence {
    Set,
    Cur,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    NotDirectory,
    IsDirectory,
    InvalidInput,
    PermissionDenied,
    AlreadyExists,
    Busy,
    NotSupported,
    Io,
}

impl FsError {
    pub const fn errno(self) -> i32 {
        match self {
            FsError::NotFound => crate::syscall::ENOENT,
            FsError::NotDirectory => crate::syscall::ENOTDIR,
            FsError::IsDirectory => crate::syscall::EISDIR,
            FsError::InvalidInput => crate::syscall::EINVAL,
            FsError::PermissionDenied => crate::syscall::EPERM,
            FsError::AlreadyExists => crate::syscall::EBUSY,
            FsError::Busy => crate::syscall::EBUSY,
            FsError::NotSupported => crate::syscall::ENOSYS,
            FsError::Io => crate::syscall::EINVAL,
        }
    }
}
