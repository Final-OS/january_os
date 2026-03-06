use super::state::VirtState;
use crate::virt::error::VirtResult;
use crate::virt::stats::VirtStats;
use crate::virt::types::{VcpuId, VmId};
use crate::virt::{irq, memory, vcpu, vm};

#[derive(Debug, Clone, Copy)]
pub struct VirtManager {
    pub state: VirtState,
    pub stats: VirtStats,
}

impl VirtManager {
    pub const fn placeholder() -> Self {
        Self {
            state: VirtState::host_placeholder(),
            stats: VirtStats::placeholder(),
        }
    }

    pub fn create_vm(&self) -> VirtResult<VmId> {
        vm::create_vm()
    }

    pub fn run_vcpu(&self, vcpu_id: VcpuId) -> VirtResult<()> {
        vcpu::run(vcpu_id)
    }

    pub fn register_region(&self, base: u64, size: u64) -> VirtResult<()> {
        memory::mmio::register_region(base, size)
    }

    pub fn inject_irq(&self, vector: u8) -> VirtResult<()> {
        irq::inject_irq(vector)
    }
}
