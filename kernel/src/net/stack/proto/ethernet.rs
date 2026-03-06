use crate::net::error::{NetError, NetResult};

pub fn process_frame(_frame: &[u8]) -> NetResult<()> {
    Err(NetError::Unsupported)
}
