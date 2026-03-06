use crate::net::error::{NetError, NetResult};

pub fn process_packet(_packet: &[u8]) -> NetResult<()> {
    Err(NetError::Unsupported)
}
