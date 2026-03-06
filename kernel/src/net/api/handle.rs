use crate::net::types::{AddressFamily, SocketType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceHandle {
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketHandle {
    pub family: AddressFamily,
    pub socket_type: SocketType,
    pub protocol: u16,
}
