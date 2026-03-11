use super::core::state::VirtState;
use super::error::{VirtError, VirtResult};

pub fn init_early() -> VirtResult<()> {
    Ok(())
}

pub fn init_core() -> VirtResult<()> {
    Ok(())
}

pub fn init_late() -> VirtResult<VirtState> {
    Err(VirtError::Unsupported)
}
