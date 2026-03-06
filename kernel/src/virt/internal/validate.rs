use crate::virt::error::{VirtError, VirtResult};

pub fn nonzero(value: u64) -> VirtResult<()> {
    if value == 0 {
        return Err(VirtError::NotReady);
    }
    Ok(())
}
