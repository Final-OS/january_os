use super::core::state::VirtState;
use super::error::VirtResult;

pub fn init_early() -> VirtResult<()> {
    Ok(())
}

pub fn init_core() -> VirtResult<()> {
    Ok(())
}

pub fn init_late() -> VirtResult<VirtState> {
    Ok(VirtState::host_placeholder())
}
