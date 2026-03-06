use crate::component::ComponentStats;
use crate::mm::MmComponentReport;

pub fn component_stats() -> ComponentStats {
    ComponentStats::ready()
}

pub fn component_report() -> MmComponentReport {
    let snapshot = crate::mm::snapshot();
    MmComponentReport {
        page_levels: snapshot.page_levels,
        va_bits: snapshot.va_bits,
        direct_map_start: snapshot.direct_map_start,
        direct_map_end: snapshot.direct_map_end,
        vmalloc_start: snapshot.vmalloc_start,
        vmalloc_end: snapshot.vmalloc_end,
    }
}
