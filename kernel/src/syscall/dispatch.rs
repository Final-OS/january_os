use super::abi::{domain_for_number, SyscallDomain};
use super::{arch, SyscallArgs, SyscallRet};

pub fn dispatch(args: &SyscallArgs) -> SyscallRet {
    match domain_for_number(args.nr) {
        SyscallDomain::Fs | SyscallDomain::Mm | SyscallDomain::Task | SyscallDomain::Unknown => {
            arch::interface().dispatch(args)
        }
        SyscallDomain::Net => crate::net::syscall::dispatch(args),
        SyscallDomain::Security => crate::security::syscall::dispatch(args),
        SyscallDomain::Virt => crate::virt::dispatch_syscall(args),
    }
}
