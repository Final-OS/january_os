use crate::virt::error::{VirtError, VirtResult};

pub fn query_runtime() -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
