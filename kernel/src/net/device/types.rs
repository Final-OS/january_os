#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueId(pub u16);

#[derive(Debug, Clone, Copy)]
pub struct NetDevice {
    pub name: &'static str,
    pub mtu: usize,
    pub link: LinkState,
}

impl NetDevice {
    pub const fn up(name: &'static str, mtu: usize) -> Self {
        Self {
            name,
            mtu,
            link: LinkState::Up,
        }
    }
}
