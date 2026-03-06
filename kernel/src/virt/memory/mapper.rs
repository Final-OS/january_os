use crate::virt::error::{VirtError, VirtResult};

pub fn map_guest_memory() -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
