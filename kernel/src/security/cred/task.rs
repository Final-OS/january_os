use crate::security::cred::Credentials;

pub fn task_credentials_placeholder() -> Credentials {
    Credentials::kernel()
}
