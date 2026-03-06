use crate::syscall::SyscallArgs;

pub fn dispatch(_args: &SyscallArgs) -> usize {
    let _ = (
        super::audit::audit_status_entry as fn() -> usize,
        super::capability::capability_query_entry as fn() -> usize,
        super::cred::cred_status_entry as fn() -> usize,
    );
    crate::security::errno_not_supported()
}
