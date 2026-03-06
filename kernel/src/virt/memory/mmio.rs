use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::MmioRegion;

pub fn register_region(base: u64, size: u64) -> VirtResult<()> {
    validate_region(MmioRegion { base, size })?;
    Err(VirtError::Unsupported)
}

pub fn validate_region(region: MmioRegion) -> VirtResult<()> {
    if region.size == 0 || region.base.checked_add(region.size).is_none() {
        return Err(VirtError::InvalidRegion);
    }
    Ok(())
}
