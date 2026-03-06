use crate::error::KernelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityError {
    Unsupported,
    AccessDenied,
    InvalidCredentials,
    PolicyDeferred,
}

pub type SecurityResult<T> = core::result::Result<T, SecurityError>;

impl SecurityError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsupported => "security operation unsupported",
            Self::AccessDenied => "security access denied",
            Self::InvalidCredentials => "invalid credentials",
            Self::PolicyDeferred => "security policy deferred",
        }
    }

    pub const fn into_kernel_error(self) -> KernelError {
        match self {
            Self::Unsupported => KernelError::NotSupported,
            Self::AccessDenied => KernelError::AccessDenied,
            Self::InvalidCredentials => KernelError::InvalidParam,
            Self::PolicyDeferred => KernelError::TryAgain,
        }
    }
}
