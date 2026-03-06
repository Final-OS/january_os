use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::VmId;

pub fn get(_vm_id: VmId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
