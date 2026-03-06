use crate::net::types::NetState;

#[derive(Debug, Clone, Copy)]
pub struct NetRuntimeState {
    pub ready: NetState,
}

impl NetRuntimeState {
    pub const fn placeholder() -> Self {
        Self {
            ready: NetState {
                device_ready: false,
                socket_ready: false,
                stack_ready: false,
            },
        }
    }
}
