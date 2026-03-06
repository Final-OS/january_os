use super::{arch, SyscallArgs, SyscallRet};

pub fn dispatch(args: &SyscallArgs) -> SyscallRet {
    arch::interface().dispatch(args)
}
