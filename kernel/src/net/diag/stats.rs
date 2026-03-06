use crate::component::ComponentState;
use crate::net::types::NetStats;

pub fn runtime_component_state() -> ComponentState {
    crate::component::ComponentStats::unsupported().state
}

pub fn component_stats() -> NetStats {
    NetStats::placeholder()
}
