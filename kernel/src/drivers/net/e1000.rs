use crate::net::error::{NetError, NetResult};

pub fn probe() -> NetResult<()> {
    Err(NetError::Unsupported)
}
