use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::VcpuId;

pub fn create(_vm_id: crate::virt::types::VmId) -> VirtResult<VcpuId> {
    Err(VirtError::Unsupported)
}

pub fn run(_vcpu_id: VcpuId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}

pub fn pause(_vcpu_id: VcpuId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}

pub fn resume(_vcpu_id: VcpuId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
