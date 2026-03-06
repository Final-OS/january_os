use super::{fail, mm_step, pass};
use crate::mm;
use core::sync::atomic::Ordering;

pub(super) fn run() {
    mm_step("status_readonly: case=snapshot_does_not_mutate_zone_counters");
    for zone_type in mm::ZoneType::iter() {
        let zone = mm::get_zone(zone_type);
        if !zone.initialized {
            continue;
        }
        let before = zone.nr_free_pages();
        let _ = zone.free_pages_snapshot_locked();
        let after = zone.nr_free_pages();
        if before != after {
            return fail(
                "status_readonly",
                "free_pages_snapshot_locked should not mutate zone counter",
            );
        }
    }

    mm_step("status_readonly: case=scrub_repairs_mismatch");
    let mut target = None;
    for zone_type in mm::ZoneType::iter() {
        let zone = mm::get_zone(zone_type);
        if zone.initialized {
            target = Some(zone_type);
            break;
        }
    }
    let zone_type = match target {
        Some(z) => z,
        None => return pass("status_readonly"),
    };

    let mut zone = mm::get_zone(zone_type);
    let before = zone.nr_free_pages();
    let injected = if before == 0 {
        1
    } else {
        before.saturating_add(1)
    };
    zone.free_pages.store(injected, Ordering::Relaxed);
    let (_observed, recomputed, repaired) = zone.scrub_free_pages_locked();
    if !repaired {
        return fail("status_readonly", "scrub should repair injected mismatch");
    }
    if zone.nr_free_pages() != recomputed {
        return fail(
            "status_readonly",
            "scrub should write recomputed value back to free_pages",
        );
    }
    let (_observed2, _recomputed2, repaired2) = zone.scrub_free_pages_locked();
    if repaired2 {
        return fail(
            "status_readonly",
            "second scrub should be idempotent when no mismatch exists",
        );
    }

    pass("status_readonly");
}
