use super::{Capability, SecurityAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileOpenRequest {
    pub inode: u64,
    pub mask: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketCreateRequest {
    pub domain: u16,
    pub socket_type: u16,
    pub protocol: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSignalRequest {
    pub target_pid: u32,
    pub signal: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityCheckRequest {
    pub capability: Capability,
    pub action: SecurityAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditWriteRequest {
    pub action: SecurityAction,
    pub allowed: bool,
}
