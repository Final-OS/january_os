use crate::error::KernelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtError {
    Unsupported,
    NotReady,
    InvalidVm,
    InvalidVcpu,
    InvalidRegion,
    InvalidMemSlot,
    InvalidIrqRoute,
}

pub type VirtResult<T> = ::core::result::Result<T, VirtError>;

impl VirtError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsupported => "virtualization operation unsupported",
            Self::NotReady => "virtualization runtime not ready",
            Self::InvalidVm => "invalid virtual machine",
            Self::InvalidVcpu => "invalid virtual cpu",
            Self::InvalidRegion => "invalid mmio region",
            Self::InvalidMemSlot => "invalid memory slot",
            Self::InvalidIrqRoute => "invalid irq route",
        }
    }

    pub const fn into_kernel_error(self) -> KernelError {
        match self {
            Self::Unsupported => KernelError::NotSupported,
            Self::NotReady => KernelError::DeviceNotReady,
            Self::InvalidVm
            | Self::InvalidVcpu
            | Self::InvalidRegion
            | Self::InvalidMemSlot
            | Self::InvalidIrqRoute => KernelError::InvalidParam,
        }
    }
}
