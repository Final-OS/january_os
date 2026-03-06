use crate::security::api::PolicyDecision;
use crate::security::api::TaskSignalRequest;
use crate::security::policy::PolicyEngine;

pub fn check_task_signal(request: &TaskSignalRequest) -> PolicyDecision {
    PolicyEngine::placeholder().evaluate_task_signal(*request)
}
