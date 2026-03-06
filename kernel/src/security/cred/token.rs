use crate::security::cred::{GroupId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Credentials {
    pub uid: UserId,
    pub gid: GroupId,
    pub euid: UserId,
    pub egid: GroupId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialToken {
    pub credentials: Credentials,
    pub version: u64,
}

impl Credentials {
    pub const fn kernel() -> Self {
        Self {
            uid: UserId(0),
            gid: GroupId(0),
            euid: UserId(0),
            egid: GroupId(0),
        }
    }
}

impl CredentialToken {
    pub const fn placeholder() -> Self {
        Self {
            credentials: Credentials::kernel(),
            version: 0,
        }
    }
}
