//! 虚拟化子系统：组件入口、VMM 控制面与平台后端门面。

pub mod config;
pub mod core;
pub mod device;
pub mod error;
pub mod facade;
pub mod hypercall;
pub mod init;
pub mod internal;
pub mod irq;
pub mod memory;
pub mod platform;
pub mod service;
pub mod stats;
pub mod types;
pub mod vcpu;
pub mod vm;

use alloc::format;
use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};
use crate::error::KernelResult;
use crate::syscall::SyscallArgs;

pub use core::capability::VirtCapability;
pub use core::info::{HypervisorType, VirtInfo};
pub use core::manager::VirtManager;
pub use core::state::VirtState;
pub use stats::VirtStats;
pub use types::{IrqRouteId, MemSlotId, MmioRegion, VcpuId, VmId};

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "virt",
    stage: ComponentStage::Late,
    deps: &["acpi"],
    summary: "virtualization host/vmm control plane, detection and platform backends",
};

pub fn init_early() -> KernelResult<()> {
    init::init_early().map_err(|err| err.into_kernel_error())
}

pub fn init_core() -> KernelResult<()> {
    init::init_core().map_err(|err| err.into_kernel_error())
}

pub fn init_late() -> KernelResult<VirtState> {
    init::init_late().map_err(|err| err.into_kernel_error())
}

pub fn detect() -> VirtInfo {
    facade::detect()
}

pub fn create_vm() -> error::VirtResult<VmId> {
    facade::create_vm()
}

pub fn run_vcpu(vcpu: VcpuId) -> error::VirtResult<()> {
    facade::run_vcpu(vcpu)
}

pub fn register_region(base: u64, size: u64) -> error::VirtResult<()> {
    facade::register_region(base, size)
}

pub fn inject_irq(vector: u8) -> error::VirtResult<()> {
    facade::inject_irq(vector)
}

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}

fn runtime_component_state() -> crate::component::ComponentState {
    stats().state
}

pub fn component_stats() -> VirtStats {
    VirtStats::placeholder()
}

pub fn dump_state() -> String {
    let info = detect();
    let runtime_stats = component_stats();
    format!(
        "component={} state={:?} virtualized={} hypervisor={} init_attempts={} vm_creates={} vcpu_runs={} irq_injections={} mmio_regions={} memslots={}",
        COMPONENT.id,
        runtime_component_state(),
        info.is_virtualized,
        info.vendor_str(),
        runtime_stats.init_attempts,
        runtime_stats.vm_creates,
        runtime_stats.vcpu_runs,
        runtime_stats.irq_injections,
        runtime_stats.mmio_regions,
        runtime_stats.memslot_updates,
    )
}

pub fn dispatch_syscall(args: &SyscallArgs) -> usize {
    service::syscall::dispatch(args)
}
