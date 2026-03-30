use super::{SyscallArgs, SyscallRet, arch};

pub fn dispatch(args: &SyscallArgs) -> SyscallRet {
    arch::interface().dispatch(args)
}
