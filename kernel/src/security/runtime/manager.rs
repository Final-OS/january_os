use crate::security::api::SecurityAction;
use crate::security::diag::stats::SecurityStats;
use crate::security::error::{SecurityError, SecurityResult};
use crate::security::runtime::state::SecurityState;
use crate::security::runtime::{SecurityRuntimeRegistry, SecurityRuntimeState};

#[derive(Debug, Clone, Copy)]
pub struct SecurityManager {
    pub state: SecurityRuntimeState,
    pub stats: SecurityStats,
    pub registry: SecurityRuntimeRegistry,
}

impl SecurityManager {
    pub const fn placeholder() -> Self {
        Self {
            state: SecurityRuntimeState::placeholder(),
            stats: SecurityStats::placeholder(),
            registry: SecurityRuntimeRegistry::placeholder(),
        }
    }

    pub fn component_state(&self) -> SecurityState {
        self.state.ready
    }

    pub fn check(&self, _action: SecurityAction) -> SecurityResult<()> {
        Err(SecurityError::Unsupported)
    }
}
