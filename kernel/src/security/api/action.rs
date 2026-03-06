#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityAction {
    FileOpen,
    SocketCreate,
    TaskSignal,
    Mount,
    CapabilityCheck,
    AuditWrite,
}
