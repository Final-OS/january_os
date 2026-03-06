use crate::virt::error::{VirtError, VirtResult};

pub fn signal_irq() -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
