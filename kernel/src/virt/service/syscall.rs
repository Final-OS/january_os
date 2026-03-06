use crate::syscall::SyscallArgs;

pub fn dispatch(_args: &SyscallArgs) -> usize {
    match crate::virt::hypercall::dispatch(0, 0, 0) {
        Ok(ret) => ret,
        Err(crate::virt::error::VirtError::Unsupported) => {
            crate::syscall::err(crate::syscall::ENOSYS)
        }
        Err(_) => crate::syscall::err(crate::syscall::EINVAL),
    }
}
