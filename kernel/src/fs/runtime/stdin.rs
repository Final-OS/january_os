pub(crate) fn read_tty_byte() -> Option<u8> {
    crate::fs::syscall::read_tty_byte()
}

pub(crate) fn stdin_has_pending_input() -> bool {
    crate::fs::syscall::stdin_has_pending_input()
}

pub(crate) fn enqueue_current_stdin_waiter() -> bool {
    crate::fs::syscall::enqueue_current_stdin_waiter()
}

pub(crate) fn dequeue_current_stdin_waiter() {
    crate::fs::syscall::dequeue_current_stdin_waiter()
}

pub(crate) fn wake_stdin_waiters_if_ready() -> usize {
    crate::fs::syscall::wake_stdin_waiters_if_ready()
}
