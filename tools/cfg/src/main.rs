//! Configuration tool for january_os
//!
//! Usage:
//!   cfg get <key>              - Get a config value (dot notation: qemu.memory)
//!   cfg generate <output>      - Generate Rust config module
//!   cfg show                   - Show all config values

use serde::Deserialize;
use std::{env, fs, path::Path, process};

/// Root configuration structure
#[derive(Debug, Deserialize)]
struct Config {
    arch: ArchConfig,
    qemu: QemuConfig,
    memory: MemoryConfig,
    kernel: KernelConfig,
    user: UserConfig,
    limits: LimitsConfig,
    memory_model: MemoryModelConfig,
    iommu: IommuConfig,
    debug: DebugConfig,
    build: BuildConfig,
}

#[derive(Debug, Deserialize)]
struct ArchConfig {
    target: String,
}

#[derive(Debug, Deserialize)]
struct QemuConfig {
    memory: String,
    smp: u32,
    cpu: String,
    kvm: String,
    #[serde(default = "default_machine")]
    machine: String,
    #[serde(default)]
    iommu: bool,
}

fn default_machine() -> String {
    "i440fx".to_string()
}

#[derive(Debug, Deserialize)]
struct MemoryConfig {
    page_size: u64,
    buddy_max_order: u32,
    zone: ZoneConfig,
    pcp: PcpConfig,
}

#[derive(Debug, Deserialize)]
struct ZoneConfig {
    dma_limit: u64,
    dma32_limit: u64,
}

#[derive(Debug, Deserialize)]
struct PcpConfig {
    high_watermark: u32,
    batch_size: u32,
}

#[derive(Debug, Deserialize)]
struct KernelConfig {
    phys_base: String,
    direct_map_offset: String,
    heap_init_size: u64,
    stack_size: u64,
}

#[derive(Debug, Deserialize)]
struct UserConfig {
    space_start: String,
    space_end: String,
    stack_top: String,
    stack_size: u64,
    stack_init_pages: u64,
    mmap_base: String,
}

#[derive(Debug, Deserialize)]
struct LimitsConfig {
    max_cpus: u32,
    #[serde(default = "default_max_apic_ids")]
    max_apic_ids: u32,
}

fn default_max_apic_ids() -> u32 {
    256
}

#[derive(Debug, Deserialize)]
struct MemoryModelConfig {
    #[serde(rename = "type")]
    model_type: String,
    numa: Option<NumaConfig>,
}

#[derive(Debug, Deserialize)]
struct NumaConfig {
    max_nodes: u32,
}

#[derive(Debug, Deserialize)]
struct IommuConfig {
    mode: String,
    translation: String,
    swiotlb_size: u64,
}

#[derive(Debug, Deserialize)]
struct DebugConfig {
    verbose: bool,
    serial: bool,
    mm_debug: bool,
    page_alloc_trace: bool,
}

#[derive(Debug, Deserialize)]
struct BuildConfig {
    opt_level: u32,
    debug_symbols: bool,
    lto: String,
}

impl Config {
    fn load(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
    }

    /// 获取架构相关的派生值
    fn arch_boot_target(&self) -> &'static str {
        match self.arch.target.as_str() {
            "x86_64" => "x86_64-unknown-uefi",
            "aarch64" => "aarch64-unknown-uefi",
            _ => "x86_64-unknown-uefi",
        }
    }

    fn arch_kernel_target(&self) -> &'static str {
        match self.arch.target.as_str() {
            "x86_64" => "x86_64-unknown-none",
            "aarch64" => "aarch64-unknown-none-softfloat",
            _ => "x86_64-unknown-none",
        }
    }

    fn arch_qemu_cmd(&self) -> &'static str {
        match self.arch.target.as_str() {
            "x86_64" => "qemu-system-x86_64",
            "aarch64" => "qemu-system-aarch64",
            _ => "qemu-system-x86_64",
        }
    }

    fn arch_efi_boot_file(&self) -> &'static str {
        match self.arch.target.as_str() {
            "x86_64" => "BOOTX64.EFI",
            "aarch64" => "BOOTAA64.EFI",
            _ => "BOOTX64.EFI",
        }
    }

    fn arch_rustup_target(&self) -> &'static str {
        match self.arch.target.as_str() {
            "x86_64" => "x86_64-unknown-uefi",
            "aarch64" => "aarch64-unknown-uefi",
            _ => "x86_64-unknown-uefi",
        }
    }

    /// Get a value by dot-notation key
    fn get(&self, key: &str) -> Option<String> {
        match key {
            // arch
            "arch.target" => Some(self.arch.target.clone()),
            // arch 派生值
            "arch.boot_target" => Some(self.arch_boot_target().to_string()),
            "arch.kernel_target" => Some(self.arch_kernel_target().to_string()),
            "arch.qemu_cmd" => Some(self.arch_qemu_cmd().to_string()),
            "arch.efi_boot_file" => Some(self.arch_efi_boot_file().to_string()),
            "arch.rustup_target" => Some(self.arch_rustup_target().to_string()),

            // qemu
            "qemu.memory" => Some(self.qemu.memory.clone()),
            "qemu.smp" => Some(self.qemu.smp.to_string()),
            "qemu.cpu" => Some(self.qemu.cpu.clone()),
            "qemu.kvm" => Some(self.qemu.kvm.clone()),
            "qemu.machine" => Some(self.qemu.machine.clone()),
            "qemu.iommu" => Some(self.qemu.iommu.to_string()),

            // memory
            "memory.page_size" => Some(self.memory.page_size.to_string()),
            "memory.buddy_max_order" => Some(self.memory.buddy_max_order.to_string()),
            "memory.zone.dma_limit" => Some(self.memory.zone.dma_limit.to_string()),
            "memory.zone.dma32_limit" => Some(self.memory.zone.dma32_limit.to_string()),
            "memory.pcp.high_watermark" => Some(self.memory.pcp.high_watermark.to_string()),
            "memory.pcp.batch_size" => Some(self.memory.pcp.batch_size.to_string()),

            // kernel
            "kernel.phys_base" => Some(self.kernel.phys_base.clone()),
            "kernel.direct_map_offset" => Some(self.kernel.direct_map_offset.clone()),
            "kernel.heap_init_size" => Some(self.kernel.heap_init_size.to_string()),
            "kernel.stack_size" => Some(self.kernel.stack_size.to_string()),

            // user
            "user.space_start" => Some(self.user.space_start.clone()),
            "user.space_end" => Some(self.user.space_end.clone()),
            "user.stack_top" => Some(self.user.stack_top.clone()),
            "user.stack_size" => Some(self.user.stack_size.to_string()),
            "user.stack_init_pages" => Some(self.user.stack_init_pages.to_string()),
            "user.mmap_base" => Some(self.user.mmap_base.clone()),

            // limits
            "limits.max_cpus" => Some(self.limits.max_cpus.to_string()),
            "limits.max_apic_ids" => Some(self.limits.max_apic_ids.to_string()),

            // memory_model
            "memory_model.type" => Some(self.memory_model.model_type.clone()),
            "memory_model.numa.max_nodes" => self
                .memory_model
                .numa
                .as_ref()
                .map(|n| n.max_nodes.to_string()),

            // iommu
            "iommu.mode" => Some(self.iommu.mode.clone()),
            "iommu.translation" => Some(self.iommu.translation.clone()),
            "iommu.swiotlb_size" => Some(self.iommu.swiotlb_size.to_string()),

            // debug
            "debug.verbose" => Some(self.debug.verbose.to_string()),
            "debug.serial" => Some(self.debug.serial.to_string()),
            "debug.mm_debug" => Some(self.debug.mm_debug.to_string()),
            "debug.page_alloc_trace" => Some(self.debug.page_alloc_trace.to_string()),

            // build
            "build.opt_level" => Some(self.build.opt_level.to_string()),
            "build.debug_symbols" => Some(self.build.debug_symbols.to_string()),
            "build.lto" => Some(self.build.lto.clone()),

            _ => None,
        }
    }

    /// Generate Rust config module
    fn generate_rust(&self) -> String {
        let numa_nodes = self
            .memory_model
            .numa
            .as_ref()
            .map(|n| n.max_nodes)
            .unwrap_or(8);
        let is_uma = self.memory_model.model_type == "uma";
        let iommu_enabled = self.iommu.mode != "off";
        let iommu_auto = self.iommu.mode == "auto";
        let iommu_passthrough = self.iommu.translation == "passthrough";
        let max_cpus = self.limits.max_cpus.max(1);
        let max_apic_ids = self.limits.max_apic_ids.max(1);
        let user_stack_init_pages = self.user.stack_init_pages.max(1);

        format!(
            r#"//! Auto-generated from os_cfg.toml - DO NOT EDIT
//! Generated by: tools/cfg

// [arch]
pub const ARCH: &str = "{}";

// [memory]
pub const PAGE_SIZE: u64 = {};
pub const PAGE_SHIFT: u64 = {};
pub const BUDDY_MAX_ORDER: usize = {};
pub const ZONE_DMA_LIMIT: u64 = {};
pub const ZONE_DMA32_LIMIT: u64 = {};
pub const PCP_HIGH_WATERMARK: u32 = {};
pub const PCP_BATCH_SIZE: u32 = {};
pub const NR_PCP_LISTS: usize = {};

// [kernel]
pub const KERNEL_PHYS_BASE: u64 = {};
pub const DIRECT_MAP_OFFSET: u64 = {};
pub const KERNEL_HEAP_INIT_SIZE: u64 = {};
pub const KERNEL_STACK_SIZE: u64 = {};

// [user]
pub const USER_SPACE_START: u64 = {};
pub const USER_SPACE_END: u64 = {};
pub const USER_STACK_TOP: u64 = {};
pub const USER_STACK_SIZE: u64 = {};
pub const USER_STACK_INIT_PAGES: u64 = {};
pub const USER_MMAP_BASE: u64 = {};

// [limits]
pub const MAX_CPUS: usize = {};
pub const MAX_APIC_IDS: usize = {};

// [memory_model]
pub const MEMORY_MODEL_UMA: bool = {};
pub const MEMORY_MODEL_NUMA: bool = {};
pub const MAX_NUMA_NODES: usize = {};

// [iommu]
pub const IOMMU_ENABLED: bool = {};
pub const IOMMU_AUTO_DETECT: bool = {};
pub const IOMMU_PASSTHROUGH: bool = {};
pub const SWIOTLB_SIZE: u64 = {};

// [debug]
pub const DEBUG_VERBOSE: bool = {};
pub const DEBUG_SERIAL: bool = {};
pub const DEBUG_MM: bool = {};
pub const DEBUG_PAGE_ALLOC_TRACE: bool = {};
"#,
            self.arch.target,
            self.memory.page_size,
            self.memory.page_size.trailing_zeros(),
            self.memory.buddy_max_order,
            self.memory.zone.dma_limit,
            self.memory.zone.dma32_limit,
            self.memory.pcp.high_watermark,
            self.memory.pcp.batch_size,
            self.memory.buddy_max_order, // NR_PCP_LISTS = MAX_ORDER
            self.kernel.phys_base,
            self.kernel.direct_map_offset,
            self.kernel.heap_init_size,
            self.kernel.stack_size,
            self.user.space_start,
            self.user.space_end,
            self.user.stack_top,
            self.user.stack_size,
            user_stack_init_pages,
            self.user.mmap_base,
            max_cpus,
            max_apic_ids,
            is_uma,
            !is_uma,
            numa_nodes,
            iommu_enabled,
            iommu_auto,
            iommu_passthrough,
            self.iommu.swiotlb_size,
            self.debug.verbose,
            self.debug.serial,
            self.debug.mm_debug,
            self.debug.page_alloc_trace,
        )
    }

    /// Show all config values
    fn show(&self) {
        println!("[arch]");
        println!("  target = {}", self.arch.target);

        println!("[qemu]");
        println!("  memory = {}", self.qemu.memory);
        println!("  smp = {}", self.qemu.smp);
        println!("  kvm = {}", self.qemu.kvm);
        println!("  machine = {}", self.qemu.machine);
        println!("  iommu = {}", self.qemu.iommu);

        println!("[memory]");
        println!("  page_size = {}", self.memory.page_size);
        println!("  buddy_max_order = {}", self.memory.buddy_max_order);
        println!("  zone.dma_limit = {}", self.memory.zone.dma_limit);
        println!("  zone.dma32_limit = {}", self.memory.zone.dma32_limit);
        println!("  pcp.high_watermark = {}", self.memory.pcp.high_watermark);
        println!("  pcp.batch_size = {}", self.memory.pcp.batch_size);

        println!("[kernel]");
        println!("  phys_base = {}", self.kernel.phys_base);
        println!("  direct_map_offset = {}", self.kernel.direct_map_offset);
        println!("  heap_init_size = {}", self.kernel.heap_init_size);
        println!("  stack_size = {}", self.kernel.stack_size);

        println!("[user]");
        println!("  space_start = {}", self.user.space_start);
        println!("  space_end = {}", self.user.space_end);
        println!("  stack_top = {}", self.user.stack_top);
        println!("  stack_size = {}", self.user.stack_size);
        println!("  stack_init_pages = {}", self.user.stack_init_pages);
        println!("  mmap_base = {}", self.user.mmap_base);

        println!("[limits]");
        println!("  max_cpus = {}", self.limits.max_cpus);
        println!("  max_apic_ids = {}", self.limits.max_apic_ids);

        println!("[memory_model]");
        println!("  type = {}", self.memory_model.model_type);
        if let Some(ref numa) = self.memory_model.numa {
            println!("  numa.max_nodes = {}", numa.max_nodes);
        }

        println!("[iommu]");
        println!("  mode = {}", self.iommu.mode);
        println!("  translation = {}", self.iommu.translation);
        println!("  swiotlb_size = {}", self.iommu.swiotlb_size);

        println!("[debug]");
        println!("  verbose = {}", self.debug.verbose);
        println!("  serial = {}", self.debug.serial);
        println!("  mm_debug = {}", self.debug.mm_debug);
        println!("  page_alloc_trace = {}", self.debug.page_alloc_trace);

        println!("[build]");
        println!("  opt_level = {}", self.build.opt_level);
        println!("  debug_symbols = {}", self.build.debug_symbols);
        println!("  lto = {}", self.build.lto);
    }
}

fn usage() {
    eprintln!("Usage: cfg <command> [args]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  get <key>         Get config value (e.g., qemu.memory)");
    eprintln!("  generate <file>   Generate Rust config module");
    eprintln!("  show              Show all config values");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  OS_CFG_PATH       Path to os_cfg.toml (default: os_cfg.toml)");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        usage();
        process::exit(1);
    }

    let cfg_path = env::var("OS_CFG_PATH").unwrap_or_else(|_| "os_cfg.toml".to_string());
    let config = match Config::load(Path::new(&cfg_path)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    match args[1].as_str() {
        "get" => {
            if args.len() < 3 {
                eprintln!("Error: missing key argument");
                process::exit(1);
            }
            match config.get(&args[2]) {
                Some(v) => println!("{}", v),
                None => {
                    eprintln!("Error: unknown key '{}'", args[2]);
                    process::exit(1);
                }
            }
        }
        "generate" => {
            if args.len() < 3 {
                eprintln!("Error: missing output file argument");
                process::exit(1);
            }
            let content = config.generate_rust();
            if let Err(e) = fs::write(&args[2], content) {
                eprintln!("Error: failed to write {}: {}", args[2], e);
                process::exit(1);
            }
            eprintln!("Generated {}", args[2]);
        }
        "show" => {
            config.show();
        }
        _ => {
            eprintln!("Error: unknown command '{}'", args[1]);
            usage();
            process::exit(1);
        }
    }
}
