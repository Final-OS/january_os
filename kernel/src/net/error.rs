use crate::error::KernelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    Unsupported,
    NotReady,
    InvalidAddress,
    InvalidSocket,
    DeviceUnavailable,
}

pub type NetResult<T> = core::result::Result<T, NetError>;

impl NetError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsupported => "network operation unsupported",
            Self::NotReady => "network runtime not ready",
            Self::InvalidAddress => "invalid network address",
            Self::InvalidSocket => "invalid socket",
            Self::DeviceUnavailable => "network device unavailable",
        }
    }

    pub const fn into_kernel_error(self) -> KernelError {
        match self {
            Self::Unsupported => KernelError::NotSupported,
            Self::NotReady => KernelError::DeviceNotReady,
            Self::InvalidAddress | Self::InvalidSocket => KernelError::InvalidParam,
            Self::DeviceUnavailable => KernelError::DeviceNotReady,
        }
    }
}
