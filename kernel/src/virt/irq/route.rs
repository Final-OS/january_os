#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqRoute {
    pub vector: u8,
    pub line: u32,
}
