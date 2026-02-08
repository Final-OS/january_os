use crate::syscall::SyscallArch;

#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "x86_64")]
pub fn interface() -> &'static dyn SyscallArch {
    &x86_64::X86_64_SYSCALL_ARCH
}
