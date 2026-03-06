#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    SysAdmin,
    NetAdmin,
    SysBoot,
    Mount,
    AuditControl,
}
