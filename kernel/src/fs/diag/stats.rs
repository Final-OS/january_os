use crate::component::ComponentStats;

pub fn component_stats() -> ComponentStats {
    crate::fs::runtime::manager::stats()
}
