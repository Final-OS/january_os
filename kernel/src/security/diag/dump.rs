use alloc::format;
use alloc::string::String;

use crate::security::diag::stats;
use crate::security::COMPONENT;

pub fn dump_state() -> String {
    let runtime_stats = stats::component_stats();
    format!(
        "component={} state={:?} init_attempts={} checks={} allowed={} denied={} deferred={} audit_events={} syscalls={}",
        COMPONENT.id,
        stats::runtime_component_state(),
        runtime_stats.init_attempts,
        runtime_stats.checks,
        runtime_stats.allowed,
        runtime_stats.denied,
        runtime_stats.deferred,
        runtime_stats.audit_events,
        runtime_stats.syscalls,
    )
}
