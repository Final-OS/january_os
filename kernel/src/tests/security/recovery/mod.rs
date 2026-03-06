use crate::{kprintln, warn};

pub fn run() {
    let first = crate::security::SecurityManager::placeholder();
    let second = crate::security::SecurityManager::placeholder();
    if first.component_state().cred_ready == second.component_state().cred_ready
        && first.component_state().policy_ready == second.component_state().policy_ready
        && first.component_state().hooks_ready == second.component_state().hooks_ready
        && first.component_state().audit_ready == second.component_state().audit_ready
        && first.component_state().syscall_ready == second.component_state().syscall_ready
    {
        kprintln!("[test/security/recovery] step=manager-recreate expected=same-placeholder-state actual=same-placeholder-state location=security::runtime::manager::placeholder");
    } else {
        warn!("[test/security/recovery] step=manager-recreate expected=same-placeholder-state actual=state-drift location=security::runtime::manager::placeholder");
    }

    match crate::security::runtime::service::install_default_policy() {
        Err(crate::security::error::SecurityError::Unsupported) => {
            kprintln!("[test/security/recovery] step=install-default-policy expected=unsupported actual=unsupported location=security::runtime::service::install_default_policy");
        }
        Ok(()) => warn!("[test/security/recovery] step=install-default-policy expected=unsupported actual=ok location=security::runtime::service::install_default_policy"),
        Err(err) => warn!("[test/security/recovery] step=install-default-policy expected=unsupported actual={:?} location=security::runtime::service::install_default_policy", err),
    }

    let stats = crate::security::component_stats();
    kprintln!("[test/security/recovery] step=component-stats action=security::component_stats expected=readable actual=init_attempts={},checks={},allowed={},denied={},deferred={},audit_events={},syscalls={} location=security::diag::stats::component_stats", stats.init_attempts, stats.checks, stats.allowed, stats.denied, stats.deferred, stats.audit_events, stats.syscalls);
}
