use super::error::VirtResult;
use super::types::{VcpuId, VmId};
use super::{irq, memory, platform, vcpu, vm, VirtInfo};

pub fn detect() -> VirtInfo {
    platform::detect()
}

pub fn create_vm() -> VirtResult<VmId> {
    vm::create_vm()
}

pub fn run_vcpu(vcpu_id: VcpuId) -> VirtResult<()> {
    vcpu::run(vcpu_id)
}

pub fn register_region(base: u64, size: u64) -> VirtResult<()> {
    memory::mmio::register_region(base, size)
}

pub fn inject_irq(vector: u8) -> VirtResult<()> {
    irq::inject_irq(vector)
}
