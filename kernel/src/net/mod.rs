//! 网络子系统组件化骨架。

pub mod api;
pub mod config;
pub mod device;
pub mod diag;
pub mod error;
pub mod runtime;
pub mod socket;
pub mod stack;
pub mod syscall;
pub mod types;

use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentState, ComponentStats};
use crate::error::{KernelError, KernelResult};
use crate::syscall::ENOSYS;

pub use api::{
    DeviceHandle, NetCapability, RecvRequest, SendRequest, SocketCreateRequest, SocketHandle,
};
pub use device::NetDevice;
pub use runtime::NetManager;
pub use socket::SocketOps;
pub use types::{
    AddressFamily, IpProtocol, Ipv4Address, Ipv6Address, MacAddress, NetState, NetStats,
    PacketBuffer, PacketDirection, PacketMetadata, ShutdownMode, SocketAddr, SocketState,
    SocketType,
};

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "net",
    stage: ComponentStage::Late,
    deps: &["drivers", "task"],
    summary: "network runtime, devices, stack and socket skeleton",
};

pub fn init_early() -> KernelResult<()> {
    runtime::init::init_early().map_err(|err| err.into_kernel_error())
}

pub fn init_core() -> KernelResult<()> {
    runtime::init::init_core().map_err(|err| err.into_kernel_error())
}

pub fn init_late() -> KernelResult<NetState> {
    runtime::init::init_late().map_err(|err| err.into_kernel_error())
}

pub fn init() -> KernelResult<NetState> {
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

pub fn component_stats() -> NetStats {
    diag::stats::component_stats()
}

pub fn dump_state() -> String {
    diag::dump::dump_state()
}

pub fn socket_create() -> Result<SocketHandle, KernelError> {
    runtime::manager::NetManager::placeholder()
        .create_socket()
        .map_err(|err| err.into_kernel_error())
}

pub fn errno_not_supported() -> usize {
    (-(ENOSYS as isize)) as usize
}
