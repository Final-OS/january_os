use crate::syscall::{err, SyscallArgs, SyscallRet, ENOSYS};

pub(crate) fn sys_ni(_args: &SyscallArgs) -> SyscallRet {
    err(ENOSYS)
}
