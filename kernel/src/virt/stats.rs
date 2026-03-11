use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::component::{ComponentState, ComponentStats};

#[derive(Debug, Clone, Copy)]
pub struct VirtStats {
    pub init_attempts: u32,
    pub vm_creates: u32,
    pub vcpu_runs: u32,
    pub irq_injections: u32,
    pub mmio_regions: u32,
    pub memslot_updates: u32,
}

impl VirtStats {
    pub const fn placeholder() -> Self {
        Self {
            init_attempts: 1,
            vm_creates: 0,
            vcpu_runs: 0,
            irq_injections: 0,
            mmio_regions: 0,
            memslot_updates: 0,
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

pub fn component_stats() -> VirtStats {
    VirtStats {
        init_attempts: INIT_ATTEMPTS.load(Ordering::Acquire),
        vm_creates: 0,
        vcpu_runs: 0,
        irq_injections: 0,
        mmio_regions: 0,
        memslot_updates: 0,
    }
}
