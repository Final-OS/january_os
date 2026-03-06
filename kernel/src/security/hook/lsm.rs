use crate::security::api::{
    FileOpenRequest, PolicyDecision, SecurityAction, SocketCreateRequest, TaskSignalRequest,
};

pub trait SecurityHook {
    fn check_action(&self, _action: SecurityAction) -> PolicyDecision {
        PolicyDecision::Defer
    }

    fn check_file_open(&self, _request: &FileOpenRequest) -> PolicyDecision {
        PolicyDecision::Defer
    }

    fn check_socket_create(&self, _request: &SocketCreateRequest) -> PolicyDecision {
        PolicyDecision::Defer
    }

    fn check_task_signal(&self, _request: &TaskSignalRequest) -> PolicyDecision {
        PolicyDecision::Defer
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NoopSecurityHook;

impl SecurityHook for NoopSecurityHook {}
