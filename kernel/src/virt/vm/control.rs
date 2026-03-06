use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::VmId;

pub fn reset(_vm_id: VmId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
