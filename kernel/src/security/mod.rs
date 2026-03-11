//! 安全子系统组件化骨架。

pub mod api;
pub mod audit;
pub mod cred;
pub mod diag;
pub mod error;
pub mod hook;
pub mod policy;
pub mod runtime;
pub mod syscall;

use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentState, ComponentStats};
use crate::error::{KernelError, KernelResult};
use crate::syscall::ENOSYS;

pub use api::{
    AuditWriteRequest, Capability, CapabilityCheckRequest, FileOpenRequest, PolicyDecision,
    SecurityAction, SocketCreateRequest, TaskSignalRequest,
};
pub use audit::AuditEvent;
pub use cred::Credentials;
pub use diag::stats::SecurityStats;
pub use runtime::{SecurityManager, SecurityState};

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "security",
    stage: ComponentStage::Late,
    deps: &["task", "fs"],
    summary: "credentials, policy hooks, audit and security syscall skeleton",
};

pub fn init_early() -> KernelResult<()> {
    runtime::init::init_early().map_err(|err| err.into_kernel_error())
}

pub fn init_core() -> KernelResult<()> {
    runtime::init::init_core().map_err(|err| err.into_kernel_error())
}

pub fn init_late() -> KernelResult<SecurityState> {
    runtime::init::init_late().map_err(|err| err.into_kernel_error())
}

pub fn init() -> KernelResult<SecurityState> {
    diag::stats::note_init_attempt();
    match init_late() {
        Ok(state) => {
            diag::stats::set_component_state(ComponentState::Ready);
            Ok(state)
        }
        Err(err) => {
            let state = if err == KernelError::NotSupported {
                ComponentState::Unsupported
            } else {
                ComponentState::Failed
            };
            diag::stats::set_component_state(state);
            Err(err)
        }
    }
}

pub fn stats() -> ComponentStats {
    diag::stats::component_runtime_stats()
}

pub fn component_stats() -> SecurityStats {
    diag::stats::component_stats()
}

pub fn dump_state() -> String {
    diag::dump::dump_state()
}

pub fn errno_not_supported() -> usize {
    (-(ENOSYS as isize)) as usize
}
