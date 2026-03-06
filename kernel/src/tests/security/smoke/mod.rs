use crate::{kprintln, warn};

pub fn run() {
    kprintln!("[test/security/smoke] step=init-early action=security::init_early expected=ok actual=running");
    match crate::security::init_early() {
        Ok(()) => kprintln!("[test/security/smoke] step=init-early-result expected=ok actual=ok location=security::runtime::init::init_early"),
        Err(err) => warn!("[test/security/smoke] step=init-early-result expected=ok actual=err({:?}) location=security::runtime::init::init_early", err),
    }

    kprintln!("[test/security/smoke] step=init-core action=security::init_core expected=ok actual=running");
    match crate::security::init_core() {
        Ok(()) => kprintln!("[test/security/smoke] step=init-core-result expected=ok actual=ok location=security::runtime::init::init_core"),
        Err(err) => warn!("[test/security/smoke] step=init-core-result expected=ok actual=err({:?}) location=security::runtime::init::init_core", err),
    }

    kprintln!("[test/security/smoke] step=init-late action=security::init_late expected=unsupported actual=running");
    match crate::security::init_late() {
        Err(crate::error::KernelError::NotSupported) => kprintln!("[test/security/smoke] step=init-late-result expected=unsupported actual=unsupported location=security::runtime::init::init_late"),
        Ok(state) => warn!("[test/security/smoke] step=init-late-result expected=unsupported actual=ok(cred_ready={},policy_ready={},hooks_ready={},audit_ready={},syscall_ready={}) location=security::runtime::init::init_late", state.cred_ready, state.policy_ready, state.hooks_ready, state.audit_ready, state.syscall_ready),
        Err(err) => warn!("[test/security/smoke] step=init-late-result expected=unsupported actual=err({:?}) location=security::runtime::init::init_late", err),
    }

    let dump = crate::security::dump_state();
    kprintln!("[test/security/smoke] step=dump-state action=security::dump_state expected=component-string actual={} location=security::diag::dump::dump_state", dump);

    let args = crate::syscall::SyscallArgs::new(500, 0, 0, 0, 0, 0, 0);
    let ret = crate::security::syscall::dispatch(&args);
    let expected = crate::syscall::err(crate::syscall::ENOSYS);
    if ret == expected {
        kprintln!("[test/security/smoke] step=syscall-dispatch-result expected=enosys actual=enosys location=security::syscall::dispatch::dispatch");
    } else {
        warn!("[test/security/smoke] step=syscall-dispatch-result expected=enosys actual={} location=security::syscall::dispatch::dispatch", ret);
    }
}
