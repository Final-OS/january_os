//! 内核初始化逻辑
//!
//! 负责按顺序初始化各个子系统。

use crate::arch;
use crate::boot::{BootInfo, BOOTINFO_MAGIC};
use crate::config;
use crate::drivers::tty::fbcon;
use crate::drivers::tty::{serial, serial_enable_rx_interrupt, SerialWriter};
use crate::drivers::{self, acpi};
use crate::fs;
use crate::interrupt;
use crate::mm::{self, MemoryRegion};
use crate::smp;
use crate::virt;
use crate::{error, info, kprint, kprintln, ok, warn};
use core::fmt::Write;

unsafe extern "C" {
    static __kernel_file_size: u8;
    static __kernel_mem_size: u8;
}

fn resolve_kernel_reserved_end_phys(info: &BootInfo) -> u64 {
    let boot_file_end = info.kernel_phys_addr.saturating_add(info.kernel_size);
    let linked_file_size = core::ptr::addr_of!(__kernel_file_size) as u64;
    let linked_mem_size = core::ptr::addr_of!(__kernel_mem_size) as u64;
    let linked_mem_end = info.kernel_phys_addr.saturating_add(linked_mem_size);
    let reserved_end = mm::page_align_up(boot_file_end.max(linked_mem_end));

    kprintln!(
        "[diag][boot] kernel reserve range=[{:#x}, {:#x}) boot_file_size={:#x} linked_file_size={:#x} linked_mem_size={:#x}",
        info.kernel_phys_addr,
        reserved_end,
        info.kernel_size,
        linked_file_size,
        linked_mem_size,
    );

    reserved_end
}

fn resolve_direct_map_offset(boot_direct_map: u64) -> u64 {
    const KERNEL_ADDR_MIN: u64 = 0xFFFF_0000_0000_0000;

    if boot_direct_map >= KERNEL_ADDR_MIN {
        return boot_direct_map;
    }

    warn!(
        "BootInfo direct_map_offset is invalid ({:#x}), fallback to config ({:#x})",
        boot_direct_map,
        config::DIRECT_MAP_OFFSET
    );
    config::DIRECT_MAP_OFFSET
}

/// 初始化内核各子系统
///
/// # Returns
/// 返回初始化后的 ACPI 配置（包含 CPU 数量等信息）
pub fn init_kernel(info: &BootInfo) {
    // 1. 串口初始化 (最早进行，以便输出调试信息)
    serial::init();

    // 2. 验证 BootInfo
    if info.magic != BOOTINFO_MAGIC {
        panic!("Invalid BootInfo magic: {:#x}", info.magic);
    }

    let direct_map = resolve_direct_map_offset(info.direct_map_offset);
    kprintln!(
        "[diag][boot] bootinfo: mem_entries={} usable={}MB direct_map={:#x} rsdp={:#x}",
        info.memory_map_entries,
        info.usable_memory / 1024 / 1024,
        direct_map,
        info.acpi_rsdp_addr,
    );

    // 3. 初始化 Framebuffer 控制台
    init_graphics(info, direct_map);

    // 清屏并打印 Banner
    kprint!("\x1b[2J\x1b[1;1H"); // Clear screen, move cursor to 1,1
    kprintln!("\n\x1b[36;1m   January OS \x1b[0;36mv0.1.0\x1b[0m");
    kprintln!("\x1b[90m   --------------------------------\x1b[0m\n");

    info!("Booting kernel...");

    // 早期 ACPI 初始化 (为了获取 SRAT 表进行 NUMA 初始化)
    if info.acpi_rsdp_addr != 0 {
        let _ = drivers::acpi::init(info.acpi_rsdp_addr);
    }

    // 4. 内存管理初始化
    kprintln!("[diag][boot] step4: init_memory begin");
    init_memory(info, direct_map);
    kprintln!("[diag][boot] step4: init_memory done");

    let kernel_stack_top = arch::current_stack_top();
    kprintln!("[diag][boot] kernel_stack_top={:#x}", kernel_stack_top);

    // 5. ACPI 解析
    kprintln!("[diag][boot] step5: init_acpi begin");
    let acpi_config = init_acpi(info);
    let cpu_count = acpi_config.cpu_count;
    kprintln!(
        "[diag][boot] step5: init_acpi done cpu_count={} lapic={:#x} ioapic={:#x}",
        cpu_count,
        acpi_config.local_apic_addr,
        acpi_config.ioapic_addr,
    );

    // 6. 初始化 PCP (Per-CPU Pages) - 依赖 CPU 数量
    kprintln!("[diag][boot] step6: init_pcp nr_cpus={}", cpu_count);
    mm::init_pcp(cpu_count as u32);

    // 7. 中断控制器初始化
    kprintln!("[diag][boot] step7: init_interrupts begin");
    init_interrupts(&acpi_config, kernel_stack_top, direct_map);
    kprintln!("[diag][boot] step7: init_interrupts done");

    // 8. 启动 AP 核心 (SMP)
    kprintln!(
        "[diag][boot] step8: smp::init begin expected_cpus={}",
        cpu_count
    );
    smp::init(direct_map, cpu_count as usize);
    kprintln!(
        "[diag][boot] step8: smp::init done online_cpus={}",
        smp::cpu_count()
    );

    // 9. IOMMU 初始化
    kprintln!("[diag][boot] step9: init_iommu begin");
    init_iommu();
    kprintln!("[diag][boot] step9: init_iommu done");

    // 9a. 虚拟化环境探测（为后续虚拟化组件留接口）
    kprintln!("[diag][boot] step9a: detect_virtualization begin");
    detect_virtualization();
    kprintln!("[diag][boot] step9a: detect_virtualization done");

    // 10. 设备驱动初始化
    kprintln!("[diag][boot] step10: init_drivers begin");
    init_drivers();
    kprintln!("[diag][boot] step10: init_drivers done");

    // 11. 启用时钟和中断
    kprintln!("[diag][boot] step11: init_timer_and_enable_interrupts begin");
    init_timer_and_enable_interrupts();
    let if_after_step11 = interrupt::interrupts_enabled();
    kprintln!(
        "[diag][boot] step11: interrupts_enabled={}",
        if_after_step11
    );

    // 11a. 初始化最小文件后端
    kprintln!("[diag][boot] step11a: fs::init begin");
    fs::init();
    kprintln!("[diag][boot] step11a: fs::init done");

    // 12. 初始化任务子系统
    kprintln!("[diag][boot] step12: task::init begin");
    crate::task::init();
    kprintln!("[diag][boot] step12: task::init done");

    kprintln!();
    ok!("Kernel initialization complete.");
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
        fbcon::init(
            fb_virt_addr,
            fb.width,
            fb.height,
            fb.stride,
            fb.pixel_format,
        );
    }
}

/// 检测 NUMA 节点信息
fn detect_numa_nodes() -> ([mm::numa::NumaNodeInfo; mm::numa::MAX_NUMNODES], usize) {
    let mut nodes = [mm::numa::NumaNodeInfo {
        node_id: 0,
        start_addr: 0,
        size: 0,
        cpu_mask: 0,
    }; mm::numa::MAX_NUMNODES];
    let mut node_count = 0;

    // 尝试从 SRAT 获取信息
    if let Some(srat) = drivers::acpi::find_table::<drivers::acpi::Srat>() {
        let mut max_node_id = 0;

        for entry in srat.entries() {
            match entry {
                drivers::acpi::SratEntry::MemoryAffinity(mem) => {
                    if mem.is_enabled() {
                        let node_id = mem.proximity_domain as usize;
                        if node_id < mm::numa::MAX_NUMNODES {
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
                        if node_id < mm::numa::MAX_NUMNODES && (apic.apic_id as usize) < 64 {
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
    info!("Initializing Memory Management...");

    let mem_regions = info.memory_map_addr as *const MemoryRegion;
    let entries_count = info.memory_map_entries as usize;
    let kernel_end_phys = resolve_kernel_reserved_end_phys(info);
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
    let max_managed = max_phys_addr.min(4 * 1024 * 1024 * 1024);
    let max_pfn = max_managed / 4096;

    // 构建内存区域信息
    const MAX_REGIONS: usize = 64;
    let mut region_infos: [mm::MemoryRegionInfo; MAX_REGIONS] = [mm::MemoryRegionInfo {
        phys_start: 0,
        page_count: 0,
        is_usable: false,
    }; MAX_REGIONS];
    let mut region_info_count = 0usize;
    for i in 0..entries_count.min(MAX_REGIONS) {
        let region = unsafe { &*mem_regions.add(i) };
        region_infos[region_info_count] = mm::MemoryRegionInfo {
            phys_start: region.phys_start,
            page_count: region.page_count,
            is_usable: region.region_type == 0,
        };
        region_info_count += 1;
    }

    // Memblock
    unsafe {
        mm::init_memblock(
            &region_infos[..region_info_count],
            info.kernel_phys_addr,
            kernel_end_phys,
        )
        .expect("Memblock init failed");

        // Buddy System
        mm::init_buddy_system(&region_infos[..region_info_count], max_pfn, direct_map)
            .expect("Buddy init failed");

        // SLUB
        mm::init_slub().expect("SLUB init failed");
        mm::finish_mm_init();

        // 初始化堆
        if let Some(heap_page) = mm::alloc_pages(8, mm::GFP_KERNEL) {
            let heap_phys = mm::page_to_pfn(heap_page) * 4096;
            let heap_virt = direct_map + heap_phys;
            mm::init_heap(heap_virt as usize, 256 * 4096);
        }
    }

    // 初始化其他内存组件
    mm::init_vma();

    // 根据配置初始化内存模型
    if config::MEMORY_MODEL_NUMA {
        let (nodes, count) = detect_numa_nodes();
        if count > 0 {
            ok!("Detected {} NUMA nodes from SRAT.", count);
        } else {
            warn!("SRAT not found or empty. Using UMA.");
        }
        unsafe {
            mm::init_numa(&nodes[..count]);
        }
    } else {
        mm::init_uma();
    }

    // 初始化 vmalloc
    {
        use alloc::boxed::Box;
        let pml4_phys = info.pml4_phys_addr;
        let pt_mgr = unsafe { mm::paging::PageTableManager::new(pml4_phys, direct_map) };
        let pt_mgr_ptr = Box::leak(Box::new(pt_mgr));
        unsafe { mm::vmalloc::init_vmalloc(direct_map, pt_mgr_ptr) };
    }

    ok!("Memory subsystems initialized (Buddy, SLUB, VMA).");
}

fn init_acpi(info: &BootInfo) -> acpi::AcpiConfig {
    info!("Parsing ACPI Tables...");

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
            "ACPI: {} CPUs | APIC: {:#x} | IOMMU: {}",
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
    info!("Initializing Interrupt Controller...");

    let int_info = interrupt::InterruptInitInfo {
        kernel_stack_top,
        local_apic_addr: acpi_config.local_apic_addr,
        ioapic_addr: acpi_config.ioapic_addr,
        ioapic_gsi_base: acpi_config.ioapic_gsi_base,
        direct_map_base: direct_map,
    };

    unsafe {
        interrupt::init(&int_info).expect("Interrupt init failed");
    }

    ok!(
        "Local APIC: {:#x} | I/O APIC: {:#x}",
        acpi_config.local_apic_addr,
        acpi_config.ioapic_addr
    );
}

fn init_iommu() {
    info!("Initializing IOMMU...");
    mm::init_iommu();
    let stats = mm::iommu_stats();
    let iommu_type = match stats.iommu_type {
        mm::IommuType::IntelVtd => "Intel VT-d",
        mm::IommuType::AmdVi => "AMD-Vi",
        mm::IommuType::Swiotlb => "SWIOTLB",
        mm::IommuType::None => "None",
    };
    let trans_mode = match stats.translation_mode {
        mm::TranslationMode::Passthrough => "Passthrough",
        mm::TranslationMode::Translate => "Translate",
    };
    ok!("IOMMU: {} | Mode: {}", iommu_type, trans_mode);
}

fn detect_virtualization() {
    let virt = virt::detect();
    if !virt.is_virtualized {
        info!("Virtualization: Bare metal");
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
        "Virtualization: {} | vendor='{}' | nested={}",
        hv,
        virt.vendor_str(),
        virt.nested_supported
    );
}

fn init_drivers() {
    info!("Initializing Devices...");
    drivers::pci::init();
    drivers::usb::init();
    drivers::input::init();
    ok!("PCIe, USB and Input devices initialized.");
}

fn init_timer_and_enable_interrupts() {
    let if_step11a = interrupt::interrupts_enabled();
    kprintln!("[diag][boot] step11a: IF(before timer init)={}", if_step11a);

    // 1. 校准 TSC (System Clock)
    interrupt::calibrate_tsc();

    // 2. 校准 APIC Timer (Scheduler/Tick)
    let timer_freq = interrupt::calibrate_timer();
    const TIMER_HZ: u32 = interrupt::TIMER_TICK_HZ as u32;
    interrupt::init_apic_timer(interrupt::IRQ_TIMER, TIMER_HZ);

    // 启用串口接收中断
    serial_enable_rx_interrupt();

    let if_step11b = interrupt::interrupts_enabled();
    kprintln!("[diag][boot] step11b: IF(before sti)={}", if_step11b);

    interrupt::enable_interrupts();

    // STI 在下一条指令边界后生效，先执行一条无副作用指令再读取 IF。
    core::hint::spin_loop();

    let if_step11c = interrupt::interrupts_enabled();

    kprintln!("[diag][boot] step11c: IF(after sti)={}", if_step11c);
    mm::paging::register_tlb_shootdown_cpu();

    ok!(
        "Timer: {} MHz | Tick: {} Hz | Interrupts: ENABLED",
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
        .map(|zt| unsafe { mm::get_zone(zt) })
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
        "  \x1b[37mCPUs:\x1b[0m       \x1b[33m{}\x1b[0m (BSP APIC ID: {})",
        acpi_config.cpu_count,
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
