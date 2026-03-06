mod bind;
mod connect;
mod send_recv;
mod socket;
mod sockopt;

use crate::syscall::SyscallArgs;

pub fn dispatch(_args: &SyscallArgs) -> usize {
    let _ = (
        bind::bind_entry as fn() -> usize,
        connect::connect_entry as fn() -> usize,
        send_recv::send_entry as fn() -> usize,
        socket::socket_entry as fn() -> usize,
        sockopt::setsockopt_entry as fn() -> usize,
    );
    super::errno_not_supported()
}
