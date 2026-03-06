use crate::net::types::{AddressFamily, SocketAddr, SocketType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketCreateRequest {
    pub family: AddressFamily,
    pub socket_type: SocketType,
    pub protocol: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendRequest<'a> {
    pub destination: Option<SocketAddr>,
    pub payload: &'a [u8],
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecvRequest<'a> {
    pub source: Option<SocketAddr>,
    pub payload: &'a mut [u8],
}
