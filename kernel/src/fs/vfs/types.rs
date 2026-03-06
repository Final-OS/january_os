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
            FsError::NotFound => crate::errno::ENOENT,
            FsError::NotDirectory => crate::errno::ENOTDIR,
            FsError::IsDirectory => crate::errno::EISDIR,
            FsError::InvalidInput => crate::errno::EINVAL,
            FsError::PermissionDenied => crate::errno::EPERM,
            FsError::AlreadyExists => crate::errno::EBUSY,
            FsError::Busy => crate::errno::EBUSY,
            FsError::NotSupported => crate::errno::ENOSYS,
            FsError::Io => crate::errno::EINVAL,
        }
    }
}
