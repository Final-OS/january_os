use crate::syscall::SyscallArch;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "x86_64")]
pub fn interface() -> &'static dyn SyscallArch {
    &x86_64::X86_64_SYSCALL_ARCH
}

#[cfg(target_arch = "aarch64")]
pub fn interface() -> &'static dyn SyscallArch {
    &aarch64::AARCH64_SYSCALL_ARCH
}

#[cfg(target_arch = "riscv64")]
pub fn interface() -> &'static dyn SyscallArch {
    &riscv64::RISCV64_SYSCALL_ARCH
}
