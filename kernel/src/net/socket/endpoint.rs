use crate::net::types::SocketAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketEndpoint {
    pub local: Option<SocketAddr>,
    pub peer: Option<SocketAddr>,
}
