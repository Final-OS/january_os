#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    Stream,
    Datagram,
    Raw,
    SeqPacket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Closed,
    Created,
    Bound,
    Listening,
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownMode {
    Read,
    Write,
    Both,
}
