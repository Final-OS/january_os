use crate::net::error::{NetError, NetResult};
use crate::net::types::PacketBuffer;

pub fn transmit(_packet: PacketBuffer<'_>) -> NetResult<()> {
    Err(NetError::Unsupported)
}
