use crate::net::error::{NetError, NetResult};

pub fn create() -> NetResult<()> {
    Err(NetError::Unsupported)
}
