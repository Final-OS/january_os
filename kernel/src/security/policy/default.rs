use crate::security::api::{
    FileOpenRequest, PolicyDecision, SecurityAction, SocketCreateRequest, TaskSignalRequest,
};

#[derive(Debug, Clone, Copy)]
pub struct DefaultPolicyProvider;

impl DefaultPolicyProvider {
    pub const fn placeholder() -> Self {
        Self
    }

    pub fn check_action(&self, _action: SecurityAction) -> PolicyDecision {
        PolicyDecision::Defer
    }

    pub fn check_file_open(&self, _request: FileOpenRequest) -> PolicyDecision {
        PolicyDecision::Defer
    }

    pub fn check_socket_create(&self, _request: SocketCreateRequest) -> PolicyDecision {
        PolicyDecision::Defer
    }

    pub fn check_task_signal(&self, _request: TaskSignalRequest) -> PolicyDecision {
        PolicyDecision::Defer
    }
}
