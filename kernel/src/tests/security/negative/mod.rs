use crate::{kprintln, warn};

pub fn run() {
    let decision = crate::security::policy::PolicyEngine::placeholder()
        .evaluate_action(crate::security::SecurityAction::CapabilityCheck);
    if decision == crate::security::PolicyDecision::Defer {
        kprintln!(
            "[test/security/negative] step=policy-decision expected=defer actual=defer location=security::policy::engine::evaluate_action"
        );
    } else {
        warn!(
            "[test/security/negative] step=policy-decision expected=defer actual={:?} location=security::policy::engine::evaluate_action",
            decision
        );
    }

    match crate::security::SecurityManager::placeholder()
        .check(crate::security::SecurityAction::FileOpen)
    {
        Err(crate::security::error::SecurityError::Unsupported) => {
            kprintln!(
                "[test/security/negative] step=manager-check expected=unsupported actual=unsupported location=security::runtime::manager::check"
            );
        }
        Ok(()) => warn!(
            "[test/security/negative] step=manager-check expected=unsupported actual=ok location=security::runtime::manager::check"
        ),
        Err(err) => warn!(
            "[test/security/negative] step=manager-check expected=unsupported actual={:?} location=security::runtime::manager::check",
            err
        ),
    }

    let event = crate::security::AuditEvent {
        subsystem: "security-test",
        action: crate::security::SecurityAction::AuditWrite,
        allowed: false,
    };
    match crate::security::audit::AuditRingBuffer::placeholder().push(event) {
        Err(crate::security::error::SecurityError::Unsupported) => {
            kprintln!(
                "[test/security/negative] step=audit-ring expected=unsupported actual=unsupported location=security::audit::ring::push"
            );
        }
        Ok(written) => warn!(
            "[test/security/negative] step=audit-ring expected=unsupported actual=ok({}) location=security::audit::ring::push",
            written
        ),
        Err(err) => warn!(
            "[test/security/negative] step=audit-ring expected=unsupported actual={:?} location=security::audit::ring::push",
            err
        ),
    }
}
