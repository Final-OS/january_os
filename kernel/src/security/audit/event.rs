use crate::security::api::SecurityAction;

#[derive(Debug, Clone, Copy)]
pub struct AuditEvent {
    pub subsystem: &'static str,
    pub action: SecurityAction,
    pub allowed: bool,
}
