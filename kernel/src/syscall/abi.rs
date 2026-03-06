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

pub fn domain_for_number(nr: usize) -> SyscallDomain {
    match nr {
        0..=199 => SyscallDomain::Fs,
        200..=299 => SyscallDomain::Mm,
        300..=399 => SyscallDomain::Task,
        400..=499 => SyscallDomain::Net,
        500..=599 => SyscallDomain::Security,
        600..=699 => SyscallDomain::Virt,
        _ => SyscallDomain::Unknown,
    }
}
