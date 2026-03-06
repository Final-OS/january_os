#[derive(Debug, Clone, Copy)]
pub enum SyscallDomain {
    Fs,
    Mm,
    Task,
    Net,
    Security,
    Virt,
    Unknown,
}
