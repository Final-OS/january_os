use crate::virt::error::{VirtError, VirtResult};

pub fn invoke() -> VirtResult<usize> {
    Err(VirtError::Unsupported)
}
