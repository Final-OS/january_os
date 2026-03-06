#[derive(Debug, Clone, Copy)]
pub struct SecurityRuntimeRegistry {
    pub hooks_registered: u32,
    pub policies_registered: u32,
    pub audit_sinks_registered: u32,
}

impl SecurityRuntimeRegistry {
    pub const fn placeholder() -> Self {
        Self {
            hooks_registered: 0,
            policies_registered: 0,
            audit_sinks_registered: 0,
        }
    }
}
