use crate::component::ComponentState;

#[derive(Debug, Clone, Copy)]
pub struct SecurityStats {
    pub init_attempts: u32,
    pub checks: u64,
    pub allowed: u64,
    pub denied: u64,
    pub deferred: u64,
    pub audit_events: u64,
    pub syscalls: u64,
}

impl SecurityStats {
    pub const fn placeholder() -> Self {
        Self {
            init_attempts: 1,
            checks: 0,
            allowed: 0,
            denied: 0,
            deferred: 0,
            audit_events: 0,
            syscalls: 0,
        }
    }
}

pub fn runtime_component_state() -> ComponentState {
    crate::component::ComponentStats::unsupported().state
}

pub fn component_stats() -> SecurityStats {
    SecurityStats::placeholder()
}
