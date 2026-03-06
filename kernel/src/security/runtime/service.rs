use crate::security::error::{SecurityError, SecurityResult};

pub fn install_default_policy() -> SecurityResult<()> {
    Err(SecurityError::Unsupported)
}

pub fn audit_bootstrap() -> SecurityResult<()> {
    Err(SecurityError::Unsupported)
}
