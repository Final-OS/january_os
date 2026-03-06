use super::policy;
use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::VmId;

pub fn create_vm() -> VirtResult<VmId> {
    policy::validate_create()?;
    Err(VirtError::Unsupported)
}

pub fn start(_vm_id: VmId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}

pub fn stop(_vm_id: VmId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}

pub fn destroy(_vm_id: VmId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
