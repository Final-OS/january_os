use alloc::format;
use alloc::string::String;

use crate::task::{current_pid, current_tid, stats, COMPONENT};

pub fn dump_state() -> String {
    format!(
        "component={} state={:?} current_pid={:?} current_tid={:?}",
        COMPONENT.id,
        stats().state,
        current_pid(),
        current_tid(),
    )
}
