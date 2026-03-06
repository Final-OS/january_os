use crate::security::audit::AuditEvent;
use crate::security::error::{SecurityError, SecurityResult};

pub trait AuditSink {
    fn write(&self, _event: &AuditEvent) -> SecurityResult<()> {
        Err(SecurityError::Unsupported)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NullAuditSink;

impl AuditSink for NullAuditSink {}
