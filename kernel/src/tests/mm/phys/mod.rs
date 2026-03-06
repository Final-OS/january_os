pub mod buddy;
pub mod page_counter_guard;
pub mod pcp;
pub mod status_readonly;

pub(super) use super::{fail, mm_step, pass};

pub(super) fn run_buddy() { buddy::run(); }
pub(super) fn run_page_counter_guard() { page_counter_guard::run(); }
pub(super) fn run_pcp() { pcp::run(); }
pub(super) fn run_status_readonly() { status_readonly::run(); }
