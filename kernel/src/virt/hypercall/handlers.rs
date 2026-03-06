use crate::virt::error::{VirtError, VirtResult};

pub fn handle(_nr: usize, _arg0: usize, _arg1: usize) -> VirtResult<usize> {
    Err(VirtError::Unsupported)
}
