pub mod dump;
pub mod stats;

pub use dump::dump_state;
pub use stats::stats;

#[cfg(target_arch = "x86_64")]
pub use crate::interrupt::arch::x86_64::trap::handlers::{
    set_timer_debug, timer_debug_heartbeats, timer_ticks,
};

#[cfg(not(target_arch = "x86_64"))]
pub fn timer_ticks() -> u64 {
    0
}

#[cfg(not(target_arch = "x86_64"))]
pub fn timer_debug_heartbeats() -> u64 {
    0
}

#[cfg(not(target_arch = "x86_64"))]
pub fn set_timer_debug(_enable: bool) {}
