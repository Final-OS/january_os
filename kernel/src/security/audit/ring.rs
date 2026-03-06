use crate::security::audit::AuditEvent;
use crate::security::error::{SecurityError, SecurityResult};

#[derive(Debug, Clone, Copy)]
pub struct AuditRingBuffer {
    pub capacity: u32,
    pub queued: u32,
}

impl AuditRingBuffer {
    pub const fn placeholder() -> Self {
        Self {
            capacity: 0,
            queued: 0,
        }
    }

    pub fn push(&self, _event: AuditEvent) -> SecurityResult<u32> {
        Err(SecurityError::Unsupported)
    }
}
