use alloc::format;
use alloc::string::String;

use crate::task::{COMPONENT, current_pid, current_tid, stats};

pub fn dump_state() -> String {
    format!(
        "component={} state={:?} current_pid={:?} current_tid={:?}",
        COMPONENT.id,
        stats().state,
        current_pid(),
        current_tid(),
    )
}
