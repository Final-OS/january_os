use alloc::format;
use alloc::string::String;

pub fn dump_state() -> String {
    format!(
        "component={} state={:?} initialized={} apic={} timer_ticks={}",
        crate::interrupt::COMPONENT.id,
        crate::interrupt::stats().state,
        crate::interrupt::initialized(),
        crate::interrupt::apic_initialized(),
        crate::interrupt::timer_ticks(),
    )
}
