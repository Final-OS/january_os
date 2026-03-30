use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use crate::component::{ComponentState, ComponentStats};

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

static COMPONENT_STATE: AtomicU8 = AtomicU8::new(ComponentState::Registered.as_u8());
static INIT_ATTEMPTS: AtomicU32 = AtomicU32::new(0);

pub fn note_init_attempt() {
    INIT_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
}

pub fn set_component_state(state: ComponentState) {
    COMPONENT_STATE.store(state.as_u8(), Ordering::Release);
}

pub fn runtime_component_state() -> ComponentState {
    ComponentState::from_u8(COMPONENT_STATE.load(Ordering::Acquire))
}

pub fn component_runtime_stats() -> ComponentStats {
    let state = runtime_component_state();
    ComponentStats {
        state,
        registrations: 1,
        init_calls: INIT_ATTEMPTS.load(Ordering::Acquire),
        failures: u32::from(matches!(state, ComponentState::Failed)),
    }
}

pub fn component_stats() -> SecurityStats {
    SecurityStats {
        init_attempts: INIT_ATTEMPTS.load(Ordering::Acquire),
        checks: 0,
        allowed: 0,
        denied: 0,
        deferred: 0,
        audit_events: 0,
        syscalls: 0,
    }
}
