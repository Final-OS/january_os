use crate::security::api::{FileOpenRequest, SocketCreateRequest, TaskSignalRequest};
use crate::security::api::{PolicyDecision, SecurityAction};
use crate::security::policy::DefaultPolicyProvider;

#[derive(Debug, Clone, Copy)]
pub struct PolicyEngine {
    pub default_provider: DefaultPolicyProvider,
}

impl PolicyEngine {
    pub const fn placeholder() -> Self {
        Self {
            default_provider: DefaultPolicyProvider::placeholder(),
        }
    }

    pub fn evaluate_action(&self, action: SecurityAction) -> PolicyDecision {
        self.default_provider.check_action(action)
    }

    pub fn evaluate_file_open(&self, request: FileOpenRequest) -> PolicyDecision {
        self.default_provider.check_file_open(request)
    }

    pub fn evaluate_socket_create(&self, request: SocketCreateRequest) -> PolicyDecision {
        self.default_provider.check_socket_create(request)
    }

    pub fn evaluate_task_signal(&self, request: TaskSignalRequest) -> PolicyDecision {
        self.default_provider.check_task_signal(request)
    }
}
