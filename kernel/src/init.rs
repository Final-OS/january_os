//! 内核初始化逻辑
//!
//! 负责按顺序初始化各个子系统。

use crate::arch;
use crate::boot::{BootInfo, BOOTINFO_MAGIC, MAX_MEMORY_REGIONS};
use crate::config;
use crate::drivers::tty::{self, SerialWriter};
use crate::drivers::{self, acpi};
use crate::fs;
use crate::interrupt;
use crate::mm::MemoryRegion;
use crate::mm::arch as mm_arch;
use crate::mm::iommu as mm_iommu;
use crate::mm::page::numa as mm_numa;
use crate::mm::vm::layout as mm_layout;
use crate::mm::vm::layout_runtime as mm_runtime;
use crate::mm::vm::paging as mm_paging;
use crate::mm::vm::vma as mm_vma;
use crate::mm::{self, component_report};
use crate::mm::setup as mm_setup;
use crate::net;
use crate::security;
use crate::smp;
use crate::virt;
use crate::{error, info, kprint, kprintln, ok, warn};
use crate::component::{
    run_component, ComponentDescriptor as KernelComponentDescriptor,
    ComponentRegistry as KernelComponentRegistry, ComponentStage as KernelComponentStage,
};
use core::fmt::Write;

const COMPONENT_SERIAL: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "serial",
    stage: KernelComponentStage::Early,
    deps: &[],
    summary: "early diagnostic console",
};
const COMPONENT_MM_LAYOUT: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "mm_layout",
    stage: KernelComponentStage::Early,
    deps: &["serial"],
    summary: "boot memory layout import from BootInfo",
};
const COMPONENT_ACPI_PROBE: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "acpi_probe",
    stage: KernelComponentStage::Early,
    deps: &["serial", "mm_layout"],
    summary: "early ACPI table discovery for topology hints",
};
const COMPONENT_MEMORY: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "memory",
    stage: KernelComponentStage::Core,
    deps: &["mm_layout"],
    summary: "memblock buddy slub vma and vmalloc init",
};
const COMPONENT_ACPI: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "acpi",
    stage: KernelComponentStage::Core,
    deps: &["memory", "acpi_probe"],
    summary: "runtime ACPI config and topology extraction",
};
const COMPONENT_PCP: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "pcp",
    stage: KernelComponentStage::Core,
    deps: &["memory", "acpi"],
    summary: "per-cpu page cache activation",
};
const COMPONENT_IOMMU: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "iommu",
    stage: KernelComponentStage::Late,
    deps: &["memory", "acpi"],
    summary: "dma translation and iommu mode selection",
};
const COMPONENT_TIMER: KernelComponentDescriptor = KernelComponentDescriptor {
    id: "timer",
    stage: KernelComponentStage::Late,
    deps: &["interrupt", "smp"],
    summary: "tsc and local apic timer enablement",
};

unsafe extern "C" {
    static __kernel_file_size: u8;
    static __kernel_mem_size: u8;
}

fn resolve_kernel_reserved_end_phys(info: &BootInfo) -> u64 {
    let boot_file_end = info.kernel_phys_addr.saturating_add(info.kernel_size);
    let linked_file_size = core::ptr::addr_of!(__kernel_file_size) as u64;
    let linked_mem_size = core::ptr::addr_of!(__kernel_mem_size) as u64;
    let linked_mem_end = info.kernel_phys_addr.saturating_add(linked_mem_size);
    let reserved_end = mm_layout::page_align_up(boot_file_end.max(linked_mem_end));

    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] kernel reserve range=[{:#x}, {:#x}) boot_file_size={:#x} linked_file_size={:#x} linked_mem_size={:#x}",
            info.kernel_phys_addr,
            reserved_end,
            info.kernel_size,
            linked_file_size,
            linked_mem_size,
        );
    }

    reserved_end
}

/// 初始化内核各子系统
///
/// # Returns
/// 返回初始化后的 ACPI 配置（包含 CPU 数量等信息）
pub fn init_kernel(info: &BootInfo) {
    let mut components = KernelComponentRegistry::new();

    // 1. 串口初始化 (最早进行，以便输出调试信息)
    run_component(&mut components, &COMPONENT_SERIAL, tty::init_early_serial);

    // 2. 验证 BootInfo
    if info.magic != BOOTINFO_MAGIC {
        panic!("Invalid BootInfo magic: {:#x}", info.magic);
    }
    if info.version < 3 {
        panic!("BootInfo version {} is too old, need >= 3", info.version);
    }

    run_component(&mut components, &COMPONENT_MM_LAYOUT, || {
        if !mm_runtime::init_from_boot_info(info) {
            panic!("Invalid kernel layout in BootInfo");
        }
    });
    let direct_map = mm_runtime::direct_map_offset();

    // 3. 初始化 Framebuffer 控制台
    init_graphics(info, direct_map);

    // 清屏并打印 Banner
    kprint!("\x1b[2J\x1b[1;1H"); // Clear screen, move cursor to 1,1
    kprintln!("\n\x1b[36;1m   January OS \x1b[0;36mv0.1.0\x1b[0m");
    kprintln!("\x1b[90m   --------------------------------\x1b[0m\n");
    let runtime_layout = mm_runtime::snapshot();
    let boot_levels = mm_runtime::boot_reported_page_levels();
    let boot_va_bits = mm_runtime::boot_reported_va_bits();
    let hw_levels = mm_runtime::hardware_page_levels();
    let hw_va_bits = mm_runtime::hardware_va_bits();
    let boot_root = mm_runtime::boot_reported_root_phys();
    let hw_root = mm_runtime::hardware_root_phys();
    let corrected = mm_runtime::paging_corrected_by_hw();
    let root_mismatch = mm_runtime::paging_root_mismatch();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] bootinfo: mem_entries={} usable={}MB direct_map=[{:#x},{:#x}) vmalloc=[{:#x},{:#x}) boot={}/L{} runtime={}/L{} hw={}/L{} root_boot={:#x} root_hw={:#x} corrected={} root_mismatch={} rsdp={:#x}",
            info.memory_map_entries,
            info.usable_memory / 1024 / 1024,
            runtime_layout.direct_map_start,
            runtime_layout.direct_map_end,
            runtime_layout.vmalloc_start,
            runtime_layout.vmalloc_end,
            boot_va_bits,
            boot_levels,
            runtime_layout.va_bits,
            runtime_layout.page_levels,
            hw_va_bits,
            hw_levels,
            boot_root,
            hw_root,
            corrected,
            root_mismatch,
            info.acpi_rsdp_addr,
        );
    }

    info!("[BOOT] Booting kernel...");

    // 早期 ACPI 初始化 (为了获取 SRAT 表进行 NUMA 初始化)
    run_component(&mut components, &COMPONENT_ACPI_PROBE, || {
        if info.acpi_rsdp_addr != 0 {
            let _ = drivers::acpi::init(info.acpi_rsdp_addr);
        }
    });

    // 4. 内存管理初始化
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step4: init_memory begin");
    }
    run_component(&mut components, &COMPONENT_MEMORY, || init_memory(info, direct_map));
    let mm_report = component_report();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[mm] component report levels={} va_bits={} direct_map=[{:#x},{:#x}) vmalloc=[{:#x},{:#x})",
            mm_report.page_levels,
            mm_report.va_bits,
            mm_report.direct_map_start,
            mm_report.direct_map_end,
            mm_report.vmalloc_start,
            mm_report.vmalloc_end,
        );
    }
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step4: init_memory done");
    }

    let kernel_stack_top = arch::current_stack_top();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] kernel_stack_top={:#x}",
            kernel_stack_top
        );
    }

    // 5. ACPI 解析
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step5: init_acpi begin");
    }
    let acpi_config = run_component(&mut components, &COMPONENT_ACPI, || init_acpi(info));
    let cpu_count = acpi_config.cpu_count;
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step5: init_acpi done cpu_count={} lapic={:#x} ioapic={:#x}",
            cpu_count,
            acpi_config.local_apic_addr,
            acpi_config.ioapic_addr,
        );
    }

    // 6. 初始化 PCP (Per-CPU Pages) - 依赖 CPU 数量
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step6: init_pcp nr_cpus={}",
            cpu_count
        );
    }
    run_component(&mut components, &COMPONENT_PCP, || mm::pcp::init_pcp(cpu_count as u32));

    // 7. 中断控制器初始化
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step7: init_interrupts begin");
    }
    run_component(&mut components, &interrupt::COMPONENT, || {
        init_interrupts(&acpi_config, kernel_stack_top, direct_map)
    });
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step7: init_interrupts done");
    }

    // 8. 启动 AP 核心 (SMP)
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step8: smp::init begin expected_cpus={}",
            cpu_count
        );
    }
    run_component(&mut components, &smp::COMPONENT, || {
        smp::init(direct_map, cpu_count as usize)
    });
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step8: smp::init done online_cpus={}",
            smp::cpu_count()
        );
    }

    // 8a. 可选回收启动期低地址 identity-map（0..3GiB）。
    if config::KERNEL_TEARDOWN_IDENTITY_MAP {
        if config::DEBUG_VERBOSE {
            kprintln!("\x1b[90m[diag]\x1b[0m[mm] teardown_identity_map begin");
        }
        let removed_identity = mm_arch::paging::teardown_bootstrap_identity_map(direct_map);
        if config::DEBUG_VERBOSE {
            kprintln!(
                "\x1b[90m[diag]\x1b[0m[mm] teardown_identity_map removed_entries={} window_gib=3",
                removed_identity
            );
        }
    } else if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[mm] teardown_identity_map disabled by config");
    }

    // 9. IOMMU 初始化
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step9: init_iommu begin");
    }
    run_component(&mut components, &COMPONENT_IOMMU, init_iommu);
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step9: init_iommu done");
    }

    // 9a. 虚拟化环境探测（为后续虚拟化组件留接口）
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step9a: detect_virtualization begin");
    }
    run_component(&mut components, &virt::COMPONENT, detect_virtualization);
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step9a: detect_virtualization done");
    }

    // 10. 设备驱动初始化
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step10: init_drivers begin");
    }
    run_component(&mut components, &drivers::COMPONENT, init_drivers);
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step10: init_drivers done");
    }

    // 11. 启用时钟和中断
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step11: init_timer_and_enable_interrupts begin");
    }
    run_component(&mut components, &COMPONENT_TIMER, init_timer_and_enable_interrupts);
    let if_after_step11 = interrupt::interrupts_enabled();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step11: interrupts_enabled={}",
            if_after_step11
        );
    }

    // 11a. 初始化最小文件后端
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step11a: fs::init begin");
    }
    let initramfs =
        if info.version >= 5 && info.initramfs_phys_addr != 0 && info.initramfs_size != 0 {
            Some((info.initramfs_phys_addr, info.initramfs_size))
        } else {
            None
        };
    run_component(&mut components, &fs::COMPONENT, || {
        let report = fs::init_runtime(initramfs);
        if config::DEBUG_VERBOSE {
            kprintln!(
                "\x1b[90m[diag]\x1b[0m[fs] component report rootfs={} initramfs_present={}",
                report.rootfs,
                report.initramfs_present,
            );
        }
    });
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step11a: fs::init done");
    }

    // 12. 初始化任务子系统
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step12: task::init begin");
    }
    run_component(&mut components, &crate::task::COMPONENT, || {
        let report = crate::task::init_runtime();
        if config::DEBUG_VERBOSE {
            kprintln!(
                "\x1b[90m[diag]\x1b[0m[task] component report scheduler_ready={} process_runtime_ready={}",
                report.scheduler_ready,
                report.process_runtime_ready,
            );
        }
    });
    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step12: task::init done");
    }

    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step13: net::init begin");
    }
    run_component(&mut components, &net::COMPONENT, || match net::init() {
        Ok(report) => {
            if config::DEBUG_VERBOSE {
                kprintln!(
                    "\x1b[90m[diag]\x1b[0m[net] component report device_ready={} socket_ready={} stack_ready={}",
                    report.device_ready,
                    report.socket_ready,
                    report.stack_ready,
                );
            }
        }
        Err(err) => {
            warn!("[NET] skeleton active but runtime unavailable: {}", err.as_str());
        }
    });

    if config::DEBUG_VERBOSE {
        kprintln!("\x1b[90m[diag]\x1b[0m[boot] step14: security::init begin");
    }
    run_component(
        &mut components,
        &security::COMPONENT,
        || match security::init() {
            Ok(report) => {
                if config::DEBUG_VERBOSE {
                    kprintln!(
                        "\x1b[90m[diag]\x1b[0m[security] component report cred_ready={} hooks_ready={} audit_ready={}",
                        report.cred_ready,
                        report.hooks_ready,
                        report.audit_ready,
                    );
                }
            }
            Err(err) => {
                warn!("[SEC] skeleton active but runtime unavailable: {}", err.as_str());
            }
        },
    );

    kprintln!();
    ok!("[BOOT] Kernel initialization complete.");
    kprintln!();

    // 系统摘要
    print_system_summary(info, &acpi_config);
}

fn init_graphics(info: &BootInfo, direct_map: u64) {
    let fb = &info.framebuffer;

    // 先用串口输出调试信息
    // let _ = write!(SerialWriter, "\n[FB Debug] phys={:#x} size={} {}x{} stride={} fmt={}\n",
    //    fb.address, fb.size, fb.width, fb.height, fb.stride, fb.pixel_format);

    if fb.address != 0 && fb.width > 0 && fb.height > 0 {
        let fb_virt_addr = direct_map + fb.address;
        let _ = tty::init_framebuffer_console(
            fb_virt_addr,
            fb.width,
            fb.height,
            fb.stride,
            fb.pixel_format,
        );
    }
}

fn init_tty_runtime() {
    let report = tty::init_runtime();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[tty] component report serial_ready={} fbcon_ready={} pty_ready={}",
            report.serial_ready,
            report.framebuffer_console_ready,
            report.pty_ready,
        );
    }
}

/// 检测 NUMA 节点信息
fn detect_numa_nodes() -> ([mm_numa::NumaNodeInfo; mm_numa::MAX_NUMNODES], usize) {
    let mut nodes = [mm_numa::NumaNodeInfo {
        node_id: 0,
        start_addr: 0,
        size: 0,
        cpu_mask: 0,
    }; mm_numa::MAX_NUMNODES];
    let mut node_count = 0;

    // 尝试从 SRAT 获取信息
    if let Some(srat) = drivers::acpi::find_table::<drivers::acpi::Srat>() {
        let mut max_node_id = 0;

        for entry in srat.entries() {
            match entry {
                drivers::acpi::SratEntry::MemoryAffinity(mem) => {
                    if mem.is_enabled() {
                        let node_id = mem.proximity_domain as usize;
                        if node_id < mm_numa::MAX_NUMNODES {
                            let info = &mut nodes[node_id];
                            info.node_id = node_id as u32;

                            // 合并内存区域 (简单的取最小起始和最大结束)
                            if info.size == 0 {
                                info.start_addr = mem.base_address();
                                info.size = mem.length();
                            } else {
                                let current_end = info.start_addr + info.size;
                                let mem_start = mem.base_address();
                                let mem_end = mem_start + mem.length();

                                let new_start = info.start_addr.min(mem_start);
                                let new_end = current_end.max(mem_end);

                                info.start_addr = new_start;
                                info.size = new_end - new_start;
                            }

                            if node_id > max_node_id {
                                max_node_id = node_id;
                            }
                        }
                    }
                }
                drivers::acpi::SratEntry::LocalApicAffinity(apic) => {
                    if apic.is_enabled() {
                        let node_id = apic.proximity_domain() as usize;
                        if node_id < mm_numa::MAX_NUMNODES && (apic.apic_id as usize) < 64 {
                            nodes[node_id].cpu_mask |= 1 << apic.apic_id;

                            if node_id > max_node_id {
                                max_node_id = node_id;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // 将有效节点移动到数组前面
        let mut valid_idx = 0;
        for i in 0..=max_node_id {
            // 只要有内存或有 CPU 就算有效节点
            if nodes[i].size > 0 || nodes[i].cpu_mask > 0 {
                if i != valid_idx {
                    nodes[valid_idx] = nodes[i];
                }
                valid_idx += 1;
            }
        }
        node_count = valid_idx;
    }

    (nodes, node_count)
}

fn init_memory(info: &BootInfo, direct_map: u64) {
    info!("[MM] Initializing Memory Management...");

    let mem_regions = info.memory_map_addr as *const MemoryRegion;
    let entries_count = info.memory_map_entries as usize;
    let kernel_end_phys = resolve_kernel_reserved_end_phys(info);
    // 与 boot/x86_64/src/paging.rs 保持一致：
    // direct-map 最多扩展到 vmalloc 起始地址之前，避免两者虚拟地址重叠。
    let vmalloc_start = mm_runtime::vmalloc_start();
    let direct_map_span = vmalloc_start.saturating_sub(direct_map);
    if direct_map_span == 0 {
        panic!(
            "Invalid direct-map layout: direct_map={:#x} vmalloc_start={:#x}",
            direct_map, vmalloc_start
        );
    }

    // 统计内存
    let mut max_phys_addr: u64 = 0;
    for i in 0..entries_count {
        let region = unsafe { &*mem_regions.add(i) };
        let region_end = region.phys_start + region.page_count * 4096;
        if region.region_type == 0 {
            if region_end > max_phys_addr {
                max_phys_addr = region_end;
            }
        }
    }
    let max_managed = if max_phys_addr > direct_map_span {
        if config::KERNEL_MANAGE_FULL_PHYS {
            panic!(
                "Managed physical memory exceeds direct-map span: max_phys={:#x}, direct_map_span={:#x} ({} GiB). Increase direct-map window or disable kernel.layout.manage_full_phys.",
                max_phys_addr,
                direct_map_span,
                direct_map_span / 1024 / 1024 / 1024
            );
        }
        warn!(
            "Managed physical memory is capped by direct-map span (degraded mode): max_phys={:#x}, managed={:#x}, limit={} GiB",
            max_phys_addr,
            direct_map_span,
            direct_map_span / 1024 / 1024 / 1024
        );
        direct_map_span
    } else {
        max_phys_addr
    };
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[mm] managed_phys_limit max_phys={:#x} managed={:#x} direct_map={:#x}",
            max_phys_addr,
            max_managed,
            direct_map
        );
    }
    let max_pfn = max_managed / 4096;

    // 构建内存区域信息
    const MAX_REGIONS: usize = MAX_MEMORY_REGIONS;
    let mut region_infos: [mm_setup::MemoryRegionInfo; MAX_REGIONS] = [mm_setup::MemoryRegionInfo {
        phys_start: 0,
        page_count: 0,
        is_usable: false,
    }; MAX_REGIONS];
    let mut region_info_count = 0usize;
    for i in 0..entries_count.min(MAX_REGIONS) {
        let region = unsafe { &*mem_regions.add(i) };
        region_infos[region_info_count] = mm_setup::MemoryRegionInfo {
            phys_start: region.phys_start,
            page_count: region.page_count,
            is_usable: region.region_type == 0,
        };
        region_info_count += 1;
    }

    // Memblock
    unsafe {
        mm_setup::init_memblock(
            &region_infos[..region_info_count],
            info.kernel_phys_addr,
            kernel_end_phys,
        )
        .expect("Memblock init failed");

        if info.version >= 5 && info.initramfs_phys_addr != 0 && info.initramfs_size != 0 {
            mm::memblock_reserve(info.initramfs_phys_addr, info.initramfs_size)
                .expect("initramfs memblock reserve failed");
        }

        // Buddy System
        mm_setup::init_buddy_system(&region_infos[..region_info_count], max_pfn, direct_map)
            .expect("Buddy init failed");

        // SLUB
        mm_setup::init_slub().expect("SLUB init failed");
        mm_setup::finish_mm_init();

        // 初始化堆（按配置分段预热，可在运行期继续增长）
        let heap_target = config::KERNEL_HEAP_INIT_SIZE as usize;
        let heap_actual = mm::heap::init_heap(heap_target);
        if heap_actual == 0 {
            panic!("Kernel heap init failed: target={} bytes", heap_target);
        } else if heap_actual < heap_target {
            warn!(
                "Kernel heap partially initialized: target={} MiB actual={} MiB",
                heap_target / 1024 / 1024,
                heap_actual / 1024 / 1024,
            );
        } else if config::DEBUG_VERBOSE {
            ok!(
                "Kernel heap initialized: target={} MiB actual={} MiB",
                heap_target / 1024 / 1024,
                heap_actual / 1024 / 1024,
            );
        }
    }

    // 初始化其他内存组件
    mm_vma::init_vma();

    // 根据配置初始化内存模型
    if config::MEMORY_MODEL_NUMA {
        let (nodes, count) = detect_numa_nodes();
        if count > 0 {
            ok!("Detected {} NUMA nodes from SRAT.", count);
        } else {
            warn!("SRAT not found or empty. Using UMA.");
        }
        unsafe {
            mm_numa::init_numa(&nodes[..count]);
        }
    } else {
        mm_numa::init_uma();
    }

    // 初始化 vmalloc
    unsafe { mm::vmalloc::init_vmalloc(direct_map) };

    ok!("Memory subsystems initialized (Buddy, SLUB, VMA).");
}

fn init_acpi(info: &BootInfo) -> acpi::AcpiConfig {
    info!("[ACPI] Parsing ACPI Tables...");

    if info.acpi_rsdp_addr != 0 {
        // 如果尚未初始化，则初始化
        if !acpi::is_initialized() {
            if let Err(e) = acpi::init(info.acpi_rsdp_addr) {
                error!("ACPI init failed: {}, using defaults", e);
                return acpi::AcpiConfig::default();
            }
        }

        let config = acpi::detect_system_config();
        ok!(
            "[ACPI] {} CPUs | APIC: {:#x} | IOMMU: {}",
            config.cpu_count,
            config.local_apic_addr,
            if config.has_iommu { "yes" } else { "no" }
        );
        config
    } else {
        warn!("ACPI not available");
        acpi::AcpiConfig::default()
    }
}

fn init_interrupts(acpi_config: &acpi::AcpiConfig, kernel_stack_top: u64, direct_map: u64) {
    info!("[INT] Initializing Interrupt Controller...");

    let mut irq_overrides = [interrupt::IrqRouteOverride {
        source: 0,
        gsi: 0,
        level_triggered: false,
        active_low: false,
    }; 16];
    let irq_override_count = core::cmp::min(acpi_config.irq_override_count, irq_overrides.len());
    for (dst, src) in irq_overrides
        .iter_mut()
        .zip(acpi_config.irq_overrides.iter())
        .take(irq_override_count)
    {
        *dst = interrupt::IrqRouteOverride {
            source: src.source,
            gsi: src.gsi,
            level_triggered: src.level_triggered,
            active_low: src.active_low,
        };
    }

    let int_info = interrupt::InterruptInitInfo {
        kernel_stack_top,
        local_apic_addr: acpi_config.local_apic_addr,
        ioapic_addr: acpi_config.ioapic_addr,
        ioapic_gsi_base: acpi_config.ioapic_gsi_base,
        irq_override_count,
        irq_overrides,
        direct_map_base: direct_map,
    };

    unsafe {
        interrupt::init(&int_info).expect("Interrupt init failed");
    }

    ok!(
        "[INT] Local APIC: {:#x} | I/O APIC: {:#x}",
        acpi_config.local_apic_addr,
        acpi_config.ioapic_addr
    );
}

fn init_iommu() {
    info!("[IOMMU] Initializing IOMMU...");
    mm_iommu::init_iommu();
    let stats = mm_iommu::iommu_stats();
    let iommu_type = match stats.iommu_type {
        mm_iommu::IommuType::IntelVtd => "Intel VT-d",
        mm_iommu::IommuType::AmdVi => "AMD-Vi",
        mm_iommu::IommuType::Swiotlb => "SWIOTLB",
        mm_iommu::IommuType::None => "None",
    };
    let trans_mode = match stats.translation_mode {
        mm_iommu::TranslationMode::Passthrough => "Passthrough",
        mm_iommu::TranslationMode::Translate => "Translate",
    };
    ok!("[IOMMU] {} | Mode: {}", iommu_type, trans_mode);
}

fn detect_virtualization() {
    let virt = virt::detect();
    if !virt.is_virtualized {
        info!("[Virtualization] Bare metal");
        return;
    }

    let hv = match virt.hypervisor {
        virt::HypervisorType::None => "None",
        virt::HypervisorType::Kvm => "KVM",
        virt::HypervisorType::Xen => "Xen",
        virt::HypervisorType::HyperV => "Hyper-V",
        virt::HypervisorType::Vmware => "VMware",
        virt::HypervisorType::Qemu => "QEMU/TCG",
        virt::HypervisorType::Unknown => "Unknown",
    };

    ok!(
        "[Virtualization] {} | vendor='{}' | nested={}",
        hv,
        virt.vendor_str(),
        virt.nested_supported
    );
}

fn init_drivers() {
    info!("[DRV] Initializing Devices...");
    let report = drivers::init_all();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[drv] component report block={} class={} pci={} usb={} input={} net={} devices={}",
            report.block_ready,
            report.class_ready,
            report.pci_ready,
            report.usb_ready,
            report.input_ready,
            report.net_ready,
            report.net_devices_registered,
        );
    }
    ok!("[DRV] PCIe, USB and Input devices initialized.");
}

fn init_timer_and_enable_interrupts() {
    let if_step11a = interrupt::interrupts_enabled();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step11a: IF(before timer init)={}",
            if_step11a
        );
    }

    // 1. 校准 TSC (System Clock)
    interrupt::calibrate_tsc();

    // 2. 校准 APIC Timer (Scheduler/Tick)
    let timer_freq = interrupt::calibrate_timer();
    const TIMER_HZ: u32 = interrupt::TIMER_TICK_HZ as u32;
    interrupt::init_apic_timer(interrupt::IRQ_TIMER, TIMER_HZ);

    // 启用串口接收中断
    tty::enable_serial_rx();

    let if_step11b = interrupt::interrupts_enabled();
    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step11b: IF(before sti)={}",
            if_step11b
        );
    }

    interrupt::enable_interrupts();

    // STI 在下一条指令边界后生效，先执行一条无副作用指令再读取 IF。
    core::hint::spin_loop();

    let if_step11c = interrupt::interrupts_enabled();

    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[boot] step11c: IF(after sti)={}",
            if_step11c
        );
    }
    mm_paging::register_tlb_shootdown_cpu();

    ok!(
        "[Timer] {} MHz | Tick: {} Hz | Interrupts: ENABLED",
        timer_freq / 1_000_000,
        TIMER_HZ
    );
}

fn print_system_summary(info: &BootInfo, acpi_config: &acpi::AcpiConfig) {
    let total_usable = info.usable_memory; // 这里原来是重新计算的，现在直接用 BootInfo 的字段或者重新计算

    // 重新计算 usable memory (为了保持一致性)
    let mem_regions = info.memory_map_addr as *const MemoryRegion;
    let entries_count = info.memory_map_entries as usize;
    let mut total_usable_calc: u64 = 0;
    for i in 0..entries_count {
        let region = unsafe { &*mem_regions.add(i) };
        if region.region_type == 0 {
            total_usable_calc += region.page_count * 4096;
        }
    }

    let total_free = mm::ZoneType::iter()
        .map(mm::get_zone)
        .filter(|z| z.initialized)
        .map(|z| z.nr_free_pages())
        .sum::<u64>();

    kprintln!("\x1b[90m--------------------------------------------------------\x1b[0m");
    kprintln!(" \x1b[36;1mSYSTEM SUMMARY\x1b[0m");
    kprintln!("\x1b[90m--------------------------------------------------------\x1b[0m");
    kprintln!(
        "  \x1b[37mMemory:\x1b[0m     \x1b[33m{}\x1b[0m MB total, \x1b[32m{}\x1b[0m MB free",
        total_usable_calc / 1024 / 1024,
        (total_free * 4) / 1024
    );
    kprintln!(
        "  \x1b[37mCPUs:\x1b[0m       detected=\x1b[33m{}\x1b[0m online=\x1b[32m{}\x1b[0m (BSP APIC ID: {})",
        acpi_config.cpu_count,
        smp::cpu_count(),
        interrupt::local_apic_id()
    );
    kprintln!(
        "  \x1b[37mDisks:\x1b[0m      \x1b[33m{}\x1b[0m",
        info.disk_count
    );
    kprintln!();

    // Timer 测试 (已移至 shell 'test timer' 命令)
    kprintln!();
}
