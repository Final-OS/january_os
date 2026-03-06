use crate::security::api::FileOpenRequest;
use crate::security::api::PolicyDecision;
use crate::security::policy::PolicyEngine;

pub fn check_file_open(request: &FileOpenRequest) -> PolicyDecision {
    PolicyEngine::placeholder().evaluate_file_open(*request)
}
