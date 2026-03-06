#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressFamily {
    Inet,
    Inet6,
    Unix,
    Unspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress {
    pub octets: [u8; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Address {
    pub octets: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv6Address {
    pub octets: [u8; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketAddr {
    V4 { address: Ipv4Address, port: u16 },
    V6 { address: Ipv6Address, port: u16 },
    Unix,
    Unspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpProtocol {
    Icmp,
    Tcp,
    Udp,
    Raw(u8),
}
