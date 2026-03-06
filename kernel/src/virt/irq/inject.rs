use crate::virt::error::{VirtError, VirtResult};

pub fn inject_irq(vector: u8) -> VirtResult<()> {
    if vector < 32 {
        return Err(VirtError::InvalidIrqRoute);
    }
    Err(VirtError::Unsupported)
}
