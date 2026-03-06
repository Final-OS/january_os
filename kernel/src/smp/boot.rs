use crate::error::{KernelError, KernelResult};

pub fn prepare_secondary_boot() -> KernelResult<()> {
    Err(KernelError::NotSupported)
}
