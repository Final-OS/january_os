use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::component::{ComponentState, ComponentStats};
use crate::net::device::registry;
use crate::net::types::NetStats;

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

pub fn component_stats() -> NetStats {
    NetStats {
        init_attempts: INIT_ATTEMPTS.load(Ordering::Acquire),
        devices_registered: registry::registered_devices(),
        sockets_open: 0,
        packets_rx: 0,
        packets_tx: 0,
    }
}
