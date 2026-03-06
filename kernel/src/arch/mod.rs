//! 架构抽象模块
//!
//! 根据目标架构选择对应的实现。

pub mod context;
pub mod cpu;
pub mod irq;
pub mod mmu;
pub mod trap;

use alloc::format;
use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

#[cfg(target_arch = "riscv64")]
pub mod riscv64;

#[cfg(target_arch = "riscv64")]
pub use riscv64::*;

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "arch",
    stage: ComponentStage::Early,
    deps: &[],
    summary: "architecture hal and cpu/mmu/irq façades",
};

pub fn init_early() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_core() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_late() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}

pub fn dump_state() -> String {
    format!(
        "component={} state={:?} cpu={} mmu={} irq={} trap={}",
        COMPONENT.id,
        stats().state,
        cpu::stats().state as u8,
        mmu::stats().state as u8,
        irq::stats().state as u8,
        trap::stats().state as u8,
    )
}
