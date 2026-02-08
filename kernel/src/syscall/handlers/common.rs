use crate::syscall::{ENOSYS, SyscallArgs, SyscallRet, err};

pub(crate) fn sys_ni(_args: &SyscallArgs) -> SyscallRet {
    err(ENOSYS)
}
