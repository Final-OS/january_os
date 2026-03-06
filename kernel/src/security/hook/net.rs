use crate::security::api::PolicyDecision;
use crate::security::api::SocketCreateRequest;
use crate::security::policy::PolicyEngine;

pub fn check_socket_create(request: &SocketCreateRequest) -> PolicyDecision {
    PolicyEngine::placeholder().evaluate_socket_create(*request)
}
