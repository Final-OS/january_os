use crate::virt::error::{VirtError, VirtResult};
use crate::virt::types::MemSlotId;

pub fn register(_slot_id: MemSlotId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}

pub fn unregister(_slot_id: MemSlotId) -> VirtResult<()> {
    Err(VirtError::Unsupported)
}
