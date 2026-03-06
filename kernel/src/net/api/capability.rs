#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetCapability {
    Device,
    Socket,
    Routing,
    Loopback,
}
