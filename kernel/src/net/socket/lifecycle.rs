use crate::net::error::{NetError, NetResult};

pub fn close() -> NetResult<()> {
    Err(NetError::Unsupported)
}

pub fn shutdown() -> NetResult<()> {
    Err(NetError::Unsupported)
}
