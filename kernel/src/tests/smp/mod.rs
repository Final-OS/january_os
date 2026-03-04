//! SMP / IPI 测试

use crate::mm;
use crate::smp;
use crate::{error, kprintln, ok, warn};
use core::sync::atomic::{AtomicUsize, Ordering};

static SMP_STEP_SEQ: AtomicUsize = AtomicUsize::new(0);
static SMP_PROBE_SENTINEL: u64 = 0x55aa_1122_3344_7788;

fn smp_step(msg: &str) {
    let seq = SMP_STEP_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/smp][step {}] {}", seq, msg);
    }
}

fn pass(name: &str) {
    ok!("smp/{}", name);
}

fn fail(name: &str, msg: &str) {
    error!("smp/{}: {}", name, msg);
}

fn run_topology_case() -> bool {
    smp_step("topology: read detected/online cpu counters");
    let detected = smp::detected_cpu_count();
    let online = smp::cpu_count();
    let max_cfg = crate::config::MAX_CPUS;
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][topology] detected={} online={} config.max_cpus={}",
            detected,
            online,
            max_cfg
        );
    }

    smp_step("topology: validate boundary conditions");
    if detected == 0 {
        fail("topology", "detected cpu count must be >= 1");
        return false;
    }
    if online == 0 {
        fail("topology", "online cpu count must be >= 1");
        return false;
    }
    if online > detected {
        fail("topology", "online cpu count exceeds detected cpu count");
        return false;
    }
    if online > max_cfg {
        fail("topology", "online cpu count exceeds config.max_cpus");
        return false;
    }

    smp_step("topology: validate multi-cpu bringup expectation");
    if detected > 1 && online == 1 {
        fail(
            "topology",
            "detected multiple CPUs but only one CPU online (AP bringup incomplete)",
        );
        return false;
    }

    pass("topology");
    true
}

fn run_cpu_id_case() -> bool {
    smp_step("cpu_id: read current cpu id and online cpu count");
    let online = smp::cpu_count();
    let cpu_id_before = smp::current_cpu_id();
    let cpu_id_after = smp::current_cpu_id();
    let apic_ready = crate::interrupt::apic_initialized();
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][cpu_id] apic_ready={} online={} current_before={} current_after={}",
            apic_ready,
            online,
            cpu_id_before,
            cpu_id_after
        );
    }

    smp_step("cpu_id: validate current cpu id range and stability");
    if online == 0 {
        fail("cpu_id", "online cpu count must not be zero");
        return false;
    }
    if cpu_id_before >= online || cpu_id_after >= online {
        fail("cpu_id", "current cpu id is out of online cpu range");
        return false;
    }
    if cpu_id_before != cpu_id_after {
        fail(
            "cpu_id",
            "current cpu id changed unexpectedly within same probe",
        );
        return false;
    }

    pass("cpu_id");
    true
}

fn run_ipi_case() -> bool {
    smp_step("ipi: gather preconditions");
    let online = smp::cpu_count();
    let registered = mm::paging::tlb_shootdown_registered_cpu_count();
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][ipi] online_cpus={} shootdown_registered_cpus={}",
            online,
            registered
        );
    }

    smp_step("ipi: invalid input case with zero probe address");
    let (targets0, handled0, matched0) = mm::paging::run_tlb_probe_on_other_cpus(0, 0x1234);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][ipi][zero-addr] targets={} handled={} matched={} expected_matched=0",
            targets0,
            handled0,
            matched0
        );
    }
    if handled0 > targets0 {
        fail("ipi", "zero-addr probe handled count exceeds targets");
        return false;
    }
    if matched0 != 0 {
        fail("ipi", "zero-addr probe matched should be zero");
        return false;
    }

    smp_step("ipi: skip remote probe in single-cpu environment");
    if online <= 1 {
        warn!("smp/ipi: single-cpu environment, skip remote IPI probe");
        pass("ipi");
        return true;
    }

    smp_step("ipi: validate shootdown target registration");
    if registered <= 1 {
        fail(
            "ipi",
            "multiple CPUs online but shootdown registered CPU count <= 1",
        );
        return false;
    }

    smp_step("ipi: positive probe with expected value");
    let probe_addr = core::ptr::addr_of!(SMP_PROBE_SENTINEL) as u64;
    let expected = SMP_PROBE_SENTINEL;
    let (targets, handled, matched) = mm::paging::run_tlb_probe_on_other_cpus(probe_addr, expected);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][ipi][positive] addr={:#x} expected={:#x} targets={} handled={} matched={}",
            probe_addr,
            expected,
            targets,
            handled,
            matched
        );
    }
    if targets == 0 {
        fail(
            "ipi",
            "positive probe has zero targets in multi-cpu environment",
        );
        return false;
    }
    if handled != targets {
        fail("ipi", "positive probe handled count does not match targets");
        return false;
    }
    if matched != targets {
        fail("ipi", "positive probe matched count does not match targets");
        return false;
    }

    smp_step("ipi: unexpected input case with mismatched expected value");
    let wrong_expected = expected ^ 0xffff_ffff_ffff_ffff;
    let (targets_bad, handled_bad, matched_bad) =
        mm::paging::run_tlb_probe_on_other_cpus(probe_addr, wrong_expected);
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][ipi][mismatch] addr={:#x} expected={:#x} targets={} handled={} matched={} expected_matched=0",
            probe_addr,
            wrong_expected,
            targets_bad,
            handled_bad,
            matched_bad
        );
    }
    if targets_bad == 0 {
        fail(
            "ipi",
            "mismatch probe has zero targets in multi-cpu environment",
        );
        return false;
    }
    if handled_bad != targets_bad {
        fail("ipi", "mismatch probe handled count does not match targets");
        return false;
    }
    if matched_bad != 0 {
        fail("ipi", "mismatch probe unexpectedly matched value");
        return false;
    }

    pass("ipi");
    true
}

fn expected_isa_irq_route(isa_irq: u8) -> (u8, bool, bool) {
    let mut gsi = isa_irq;
    let mut level_triggered = false;
    let mut active_low = false;

    if let Some(madt) = crate::drivers::acpi::find_table::<crate::drivers::acpi::Madt>() {
        let madt_info = crate::drivers::acpi::parse_madt(madt);
        for idx in 0..madt_info.irq_override_count {
            let ov = madt_info.irq_overrides[idx];
            if ov.source != isa_irq {
                continue;
            }

            if ov.gsi <= u8::MAX as u32 {
                gsi = ov.gsi as u8;
                level_triggered = ov.level_triggered;
                active_low = ov.active_low;
            }
            break;
        }
    }

    (gsi, level_triggered, active_low)
}

fn check_irq_route(name: &str, isa_irq: u8, expect_vector: u8) -> Result<(), &'static str> {
    let (gsi, expect_level, expect_low) = expected_isa_irq_route(isa_irq);
    let Some(route) = crate::interrupt::ioapic_read_irq_route(gsi) else {
        return Err("ioapic route read returned None");
    };

    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][irq-route] {} isa_irq={} gsi={} vector={} masked={} level={} low={} dest={}",
            name,
            isa_irq,
            gsi,
            route.vector,
            route.masked,
            route.level_triggered,
            route.active_low,
            route.dest
        );
    }

    if route.vector != expect_vector {
        return Err("ioapic vector mismatch");
    }
    if route.masked {
        return Err("ioapic route should be unmasked");
    }
    if route.level_triggered != expect_level {
        return Err("ioapic trigger mode mismatch");
    }
    if route.active_low != expect_low {
        return Err("ioapic polarity mismatch");
    }
    if route.dest != 0 {
        return Err("ioapic destination APIC ID mismatch");
    }

    Ok(())
}

fn run_irq_route_case() -> bool {
    smp_step("irq_route: gather ioapic/acpi preconditions");
    let acpi_cfg = crate::drivers::acpi::detect_system_config();
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][irq-route] ioapic_addr={:#x} ioapic_gsi_base={} override_count={}",
            acpi_cfg.ioapic_addr,
            acpi_cfg.ioapic_gsi_base,
            acpi_cfg.irq_override_count
        );
    }

    if acpi_cfg.ioapic_addr == 0 {
        warn!("smp/irq_route: ioapic not present, skip route validation");
        pass("irq_route");
        return true;
    }

    smp_step("irq_route: validate keyboard route");
    if let Err(msg) = check_irq_route("keyboard", 1, crate::interrupt::IRQ_KEYBOARD) {
        fail("irq_route", msg);
        return false;
    }

    smp_step("irq_route: validate mouse route");
    if let Err(msg) = check_irq_route("mouse", 12, crate::interrupt::IRQ_MOUSE) {
        fail("irq_route", msg);
        return false;
    }

    smp_step("irq_route: validate serial route");
    if let Err(msg) = check_irq_route("serial", 4, crate::interrupt::IRQ_COM1) {
        fail("irq_route", msg);
        return false;
    }

    pass("irq_route");
    true
}

fn run_scheduler_stats_case() -> bool {
    smp_step("sched_stats: read scheduler stats before probe");
    let before = crate::task::scheduler_snapshot_stats();
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][sched_stats][before] local={} steal_attempts={} steal_successes={} idle={}",
            before.local_picks,
            before.steal_attempts,
            before.steal_successes,
            before.idle_fallbacks
        );
    }

    smp_step("sched_stats: trigger scheduler probe cycles");
    for _ in 0..8 {
        crate::task::scheduler::schedule();
    }

    smp_step("sched_stats: read scheduler stats after probe");
    let after = crate::task::scheduler_snapshot_stats();
    if crate::config::DEBUG_VERBOSE {
        kprintln!(
            "[test/smp][sched_stats][after] local={} steal_attempts={} steal_successes={} idle={}",
            after.local_picks,
            after.steal_attempts,
            after.steal_successes,
            after.idle_fallbacks
        );
    }

    if after.local_picks < before.local_picks {
        fail("sched_stats", "local picks counter regressed");
        return false;
    }
    if after.steal_attempts < before.steal_attempts {
        fail("sched_stats", "steal attempts counter regressed");
        return false;
    }
    if after.steal_successes < before.steal_successes {
        fail("sched_stats", "steal successes counter regressed");
        return false;
    }
    if after.idle_fallbacks < before.idle_fallbacks {
        fail("sched_stats", "idle fallback counter regressed");
        return false;
    }
    if after.steal_successes > after.steal_attempts {
        fail("sched_stats", "steal successes exceeds attempts");
        return false;
    }

    pass("sched_stats");
    true
}

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    SMP_STEP_SEQ.store(0, Ordering::SeqCst);
    kprintln!("=== SMP / IPI Tests ===");
    smp_step("start smp test suite");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/smp] filter={:?}", filter);
    }

    match filter {
        None | Some("all") => {
            smp_step("run case=topology");
            let _ = run_topology_case();
            smp_step("run case=cpu_id");
            let _ = run_cpu_id_case();
            smp_step("run case=ipi");
            let _ = run_ipi_case();
            smp_step("run case=irq_route");
            let _ = run_irq_route_case();
            smp_step("run case=sched_stats");
            let _ = run_scheduler_stats_case();
        }
        Some("topology") => {
            smp_step("run case=topology");
            let _ = run_topology_case();
        }
        Some("cpu_id") => {
            smp_step("run case=cpu_id");
            let _ = run_cpu_id_case();
        }
        Some("ipi") => {
            smp_step("run case=ipi");
            let _ = run_ipi_case();
        }
        Some("irq_route") => {
            smp_step("run case=irq_route");
            let _ = run_irq_route_case();
        }
        Some("sched_stats") => {
            smp_step("run case=sched_stats");
            let _ = run_scheduler_stats_case();
        }
        Some("help") | _ => {
            smp_step("show help");
            kprintln!("Usage: test smp [name]");
            kprintln!("Available smp tests:");
            kprintln!("  topology  - detected/online CPU consistency checks");
            kprintln!("  cpu_id    - current CPU ID range/stability checks");
            kprintln!("  ipi       - cross-CPU TLB probe IPI checks");
            kprintln!("  irq_route - IOAPIC route consistency with MADT overrides");
            kprintln!("  sched_stats - scheduler observability counters");
            kprintln!("  all       - run all smp tests");
            kprintln!("Note: `test smp` defaults to `all`.");
            kprintln!();
            return;
        }
    }

    smp_step("smp test suite done");
    kprintln!();
}
