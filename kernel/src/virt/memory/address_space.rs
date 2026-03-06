#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestAddressSpace {
    Gva,
    Gpa,
    Hva,
}
