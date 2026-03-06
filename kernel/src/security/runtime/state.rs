#[derive(Debug, Clone, Copy)]
pub struct SecurityState {
    pub cred_ready: bool,
    pub policy_ready: bool,
    pub hooks_ready: bool,
    pub audit_ready: bool,
    pub syscall_ready: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SecurityRuntimeState {
    pub ready: SecurityState,
}

impl SecurityRuntimeState {
    pub const fn placeholder() -> Self {
        Self {
            ready: SecurityState {
                cred_ready: false,
                policy_ready: false,
                hooks_ready: false,
                audit_ready: false,
                syscall_ready: false,
            },
        }
    }
}
