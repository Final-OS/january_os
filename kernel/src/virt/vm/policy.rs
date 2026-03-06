use crate::virt::config;
use crate::virt::error::VirtResult;

pub fn validate_create() -> VirtResult<()> {
    let _ = config::MAX_VMS;
    Ok(())
}
