#[derive(Debug, Clone, Copy)]
pub struct SocketTable {
    pub allocated: u32,
}

impl SocketTable {
    pub const fn empty() -> Self {
        Self { allocated: 0 }
    }
}
