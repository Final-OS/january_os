#[derive(Debug, Clone, Copy)]
pub struct NetState {
    pub device_ready: bool,
    pub socket_ready: bool,
    pub stack_ready: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct NetStats {
    pub init_attempts: u32,
    pub devices_registered: u32,
    pub sockets_open: u32,
    pub packets_rx: u64,
    pub packets_tx: u64,
}

impl NetStats {
    pub const fn placeholder() -> Self {
        Self {
            init_attempts: 1,
            devices_registered: 0,
            sockets_open: 0,
            packets_rx: 0,
            packets_tx: 0,
        }
    }
}
