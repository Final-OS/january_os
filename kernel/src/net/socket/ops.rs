use crate::net::error::{NetError, NetResult};

pub trait SocketOps {
    fn bind(&self) -> NetResult<()> {
        Err(NetError::Unsupported)
    }

    fn connect(&self) -> NetResult<()> {
        Err(NetError::Unsupported)
    }

    fn send(&self, _buf: &[u8]) -> NetResult<usize> {
        Err(NetError::Unsupported)
    }

    fn recv(&self, _buf: &mut [u8]) -> NetResult<usize> {
        Err(NetError::Unsupported)
    }
}
