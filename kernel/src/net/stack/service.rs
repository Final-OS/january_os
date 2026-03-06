use crate::net::error::{NetError, NetResult};

pub fn register_loopback() -> NetResult<()> {
    Err(NetError::Unsupported)
}

pub fn bring_up_default_stack() -> NetResult<()> {
    Err(NetError::Unsupported)
}
