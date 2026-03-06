pub mod fork;
pub mod usermode;
pub mod wait_reap;

pub(super) use super::task_step;

pub(super) fn run_fork() {
    fork::run();
}

pub(super) fn run_usermode() {
    usermode::run();
}

pub(super) fn run_wait_reap() {
    wait_reap::run();
}
