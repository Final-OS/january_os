use alloc::format;
use alloc::string::String;

use crate::mm::{COMPONENT, component_report, stats};

pub fn dump_state() -> String {
    let report = component_report();
    format!(
        "component={} state={:?} levels={} va_bits={} direct_map=[{:#x},{:#x}) vmalloc=[{:#x},{:#x})",
        COMPONENT.id,
        stats().state,
        report.page_levels,
        report.va_bits,
        report.direct_map_start,
        report.direct_map_end,
        report.vmalloc_start,
        report.vmalloc_end,
    )
}
