use super::abi::SyscallDomain;

#[derive(Debug, Clone, Copy)]
pub struct SyscallRoute {
    pub nr: usize,
    pub name: &'static str,
    pub domain: SyscallDomain,
}
