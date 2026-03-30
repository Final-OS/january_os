use alloc::format;
use alloc::string::String;

use crate::net::COMPONENT;
use crate::net::diag::stats;

pub fn dump_state() -> String {
    let runtime_stats = stats::component_stats();
    format!(
        "component={} state={:?} init_attempts={} devices={} sockets={} rx={} tx={}",
        COMPONENT.id,
        stats::runtime_component_state(),
        runtime_stats.init_attempts,
        runtime_stats.devices_registered,
        runtime_stats.sockets_open,
        runtime_stats.packets_rx,
        runtime_stats.packets_tx,
    )
}
