use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::{IrqRouteId, MemSlotId, MmioRegion, VcpuId, VmId};

pub trait VmOps {
    fn create_vm(&self) -> VirtResult<VmId> {
        Err(VirtError::Unsupported)
    }
}

pub trait VcpuOps {
    fn run_vcpu(&self, _vcpu_id: VcpuId) -> VirtResult<()> {
        Err(VirtError::Unsupported)
    }
}

pub trait MemoryOps {
    fn register_memslot(&self, _slot_id: MemSlotId) -> VirtResult<()> {
        Err(VirtError::Unsupported)
    }

    fn register_region(&self, _region: MmioRegion) -> VirtResult<()> {
        Err(VirtError::Unsupported)
    }
}

pub trait IrqOps {
    fn inject_irq(&self, _route_id: IrqRouteId, _vector: u8) -> VirtResult<()> {
        Err(VirtError::Unsupported)
    }
}

pub trait DeviceOps {
    fn attach_device(&self, _vm_id: VmId) -> VirtResult<()> {
        Err(VirtError::Unsupported)
    }
}
