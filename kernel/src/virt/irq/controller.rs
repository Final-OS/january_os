#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualIrqController {
    Pic,
    Apic,
    Gic,
    Aia,
}
