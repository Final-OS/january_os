use crate::security::error::{SecurityError, SecurityResult};
use crate::security::runtime::service;
use crate::security::runtime::state::SecurityState;

pub fn init_early() -> SecurityResult<()> {
    Ok(())
}

pub fn init_core() -> SecurityResult<()> {
    let _ = (
        service::install_default_policy as fn() -> SecurityResult<()>,
        service::audit_bootstrap as fn() -> SecurityResult<()>,
    );
    Ok(())
}

pub fn init_late() -> SecurityResult<SecurityState> {
    Err(SecurityError::Unsupported)
}
