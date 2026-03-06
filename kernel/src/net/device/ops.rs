use crate::net::error::{NetError, NetResult};

pub trait NetDeviceOps {
    fn transmit(&self, _buf: &[u8]) -> NetResult<usize> {
        Err(NetError::Unsupported)
    }

    fn poll_receive(&self, _buf: &mut [u8]) -> NetResult<usize> {
        Err(NetError::Unsupported)
    }
}
