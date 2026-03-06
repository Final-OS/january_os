use crate::net::error::{NetError, NetResult};
use crate::net::runtime::{NetRuntimeRegistry, NetRuntimeState};
use crate::net::types::NetStats;
use crate::net::{SocketCreateRequest, SocketHandle};

#[derive(Debug, Clone, Copy)]
pub struct NetManager {
    pub state: NetRuntimeState,
    pub stats: NetStats,
    pub registry: NetRuntimeRegistry,
}

impl NetManager {
    pub const fn placeholder() -> Self {
        Self {
            state: NetRuntimeState::placeholder(),
            stats: NetStats::placeholder(),
            registry: NetRuntimeRegistry::placeholder(),
        }
    }

    pub fn create_socket(&self) -> NetResult<SocketHandle> {
        self.create_socket_with(SocketCreateRequest {
            family: crate::net::AddressFamily::Unspecified,
            socket_type: crate::net::SocketType::Stream,
            protocol: 0,
        })
    }

    pub fn create_socket_with(&self, _request: SocketCreateRequest) -> NetResult<SocketHandle> {
        Err(NetError::Unsupported)
    }
}
