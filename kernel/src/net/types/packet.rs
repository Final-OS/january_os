#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDirection {
    Ingress,
    Egress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketMetadata {
    pub direction: PacketDirection,
    pub frame_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PacketBuffer<'a> {
    pub data: &'a [u8],
    pub metadata: PacketMetadata,
}
