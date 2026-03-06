#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioTransport {
    Mmio,
    Pci,
}
