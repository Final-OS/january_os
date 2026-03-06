use crate::net::device::registry::DeviceRegistry;
use crate::net::socket::table::SocketTable;

#[derive(Debug, Clone, Copy)]
pub struct NetRuntimeRegistry {
    pub devices: DeviceRegistry,
    pub sockets: SocketTable,
}

impl NetRuntimeRegistry {
    pub const fn placeholder() -> Self {
        Self {
            devices: DeviceRegistry::empty(),
            sockets: SocketTable::empty(),
        }
    }
}
