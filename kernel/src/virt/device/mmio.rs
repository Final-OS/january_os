use crate::virt::error::{VirtError, VirtResult};

pub fn handle_mmio_access() -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
