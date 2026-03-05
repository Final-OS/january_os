//! Unified Kernel Error Handling Framework
//!
//! This module defines the standard error type `KernelError` and `Result` alias
//! used throughout the kernel.

/// Standard Kernel Error definitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum KernelError {
    /// Success (0)
    Success = 0,

    // --- Generic Errors ---
    /// Generic failure
    Failed = -1,
    /// Invalid argument or parameter
    InvalidParam = -2,
    /// Out of memory
    NoMemory = -3,
    /// Resource busy or locked
    Busy = -4,
    /// Operation timed out
    Timeout = -5,
    /// Item not found
    NotFound = -6,
    /// Item already exists
    AlreadyExists = -7,
    /// Operation not supported
    NotSupported = -8,
    /// Permission denied
    AccessDenied = -9,

    // --- IO / Device Errors ---
    /// Generic IO error
    Io = -10,
    /// Device not ready
    DeviceNotReady = -11,
    /// Device error (hardware failure)
    DeviceError = -12,
    /// Buffer too small
    BufferTooSmall = -13,

    // --- Memory Errors ---
    /// Invalid address or alignment
    InvalidAddress = -20,
    /// Address not mapped
    NotMapped = -21,

    // --- System Errors ---
    /// Interrupted system call/operation
    Interrupted = -30,
    /// Try again
    TryAgain = -31,
}

/// Standard Kernel Result type
pub type KernelResult<T> = core::result::Result<T, KernelError>;

impl KernelError {
    /// Returns a string description of the error
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::Failed => "Generic failure",
            Self::InvalidParam => "Invalid parameter",
            Self::NoMemory => "Out of memory",
            Self::Busy => "Resource busy",
            Self::Timeout => "Operation timed out",
            Self::NotFound => "Not found",
            Self::AlreadyExists => "Already exists",
            Self::NotSupported => "Not supported",
            Self::AccessDenied => "Access denied",
            Self::Io => "I/O error",
            Self::DeviceNotReady => "Device not ready",
            Self::DeviceError => "Device error",
            Self::BufferTooSmall => "Buffer too small",
            Self::InvalidAddress => "Invalid address",
            Self::NotMapped => "Address not mapped",
            Self::Interrupted => "Interrupted",
            Self::TryAgain => "Try again",
        }
    }
}

// Conversion from layout error
impl From<core::alloc::LayoutError> for KernelError {
    fn from(_: core::alloc::LayoutError) -> Self {
        KernelError::InvalidParam
    }
}

// Conversion from core::fmt::Error
impl From<core::fmt::Error> for KernelError {
    fn from(_: core::fmt::Error) -> Self {
        KernelError::Io
    }
}
