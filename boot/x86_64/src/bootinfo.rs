//! 引导信息结构体与常量定义

/// 像素格式
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum PixelFormatType {
    /// RGB 格式 (R在低字节)
    Rgb = 0,
    /// BGR 格式 (B在低字节，最常见)
    Bgr = 1,
    /// 位掩码格式
    Bitmask = 2,
    /// 仅 BLT 格式
    BltOnly = 3,
}

/// 帧缓冲区信息
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    /// 帧缓冲区物理地址
    pub address: u64,
    /// 帧缓冲区总大小（字节）
    pub size: u64,
    /// 屏幕宽度（像素）
    pub width: u32,
    /// 屏幕高度（像素）
    pub height: u32,
    /// 每行像素数（可能 > width，因为对齐）
    pub stride: u32,
    /// 每像素字节数
    pub bytes_per_pixel: u32,
    /// 像素格式
    pub pixel_format: u32,
    /// 保留，对齐用
    pub _reserved: u32,
}

/// 内存区域类型（简化版）
#[repr(u32)]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryRegionType {
    /// 可用内存
    Usable = 0,
    /// 保留内存（不可使用）
    Reserved = 1,
    /// ACPI 可回收内存
    AcpiReclaimable = 2,
    /// ACPI NVS 内存
    AcpiNvs = 3,
    /// 内存映射 I/O
    Mmio = 4,
    /// 引导程序代码/数据（内核可回收）
    BootloaderReclaimable = 5,
    /// 内核代码/数据
    KernelAndModules = 6,
    /// 帧缓冲区
    Framebuffer = 7,
}

/// 内存区域描述符（简化版，兼容性更好）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryRegion {
    /// 物理起始地址
    pub phys_start: u64,
    /// 虚拟起始地址（通常与物理相同）
    pub virt_start: u64,
    /// 页数（每页 4KB）
    pub page_count: u64,
    /// 区域类型
    pub region_type: u32,
    /// 属性标志
    pub attributes: u32,
}

/// 磁盘类型
#[repr(u32)]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum DiskType {
    Unknown = 0,
    HardDisk = 1,
    CdRom = 2,
    Usb = 3,
    NVMe = 4,
    Floppy = 5,
    Network = 6,
}

/// 磁盘信息
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DiskInfo {
    /// 磁盘类型
    pub disk_type: u32,
    /// 是否可移动 (1=可移动, 0=固定)
    pub removable: u32,
    /// 是否为启动设备 (1=是, 0=否)
    pub boot_device: u32,
    /// 是否只读
    pub read_only: u32,
    /// 逻辑块大小（字节）
    pub block_size: u64,
    /// 总块数
    pub total_blocks: u64,
    /// 总容量（字节）
    pub total_size: u64,
    /// 媒体 ID
    pub media_id: u32,
    /// 保留
    pub _reserved: u32,
}

/// 主引导信息结构体 - 传递给内核的所有信息
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KernelVaLayout {
    /// 虚拟地址有效位宽（48 或 57）
    pub va_bits: u8,
    /// 页表层级（4 或 5）
    pub page_levels: u8,
    pub _reserved0: [u8; 6],
    /// 直接映射窗口 [start, end)
    pub direct_map_start: u64,
    pub direct_map_end: u64,
    /// vmalloc 窗口 [start, end)
    pub vmalloc_start: u64,
    pub vmalloc_end: u64,
    /// vmemmap 窗口 [start, end)
    pub vmemmap_start: u64,
    pub vmemmap_end: u64,
    /// modules 窗口 [start, end)
    pub modules_start: u64,
    pub modules_end: u64,
    /// fixmap 窗口 [start, end)
    pub fixmap_start: u64,
    pub fixmap_end: u64,
}

/// 主引导信息结构体 - 传递给内核的所有信息
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    /// 魔数，用于验证结构体有效性 (应为 0x4A414E5F4F530000 "JAN_OS\0\0")
    pub magic: u64,
    /// 结构体版本号
    pub version: u32,
    /// 结构体大小（字节）
    pub size: u32,

    // ========== 帧缓冲区信息 ==========
    pub framebuffer: FramebufferInfo,

    // ========== 内存映射 ==========
    /// 内存区域数组地址
    pub memory_map_addr: u64,
    /// 内存区域数量
    pub memory_map_entries: u32,
    /// 每个条目大小
    pub memory_map_entry_size: u32,
    /// 总可用内存（字节）
    pub total_memory: u64,
    /// 可用内存（字节）
    pub usable_memory: u64,

    // ========== ACPI 信息 ==========
    /// ACPI RSDP 地址 (0 表示未找到)
    pub acpi_rsdp_addr: u64,
    /// ACPI 版本 (1 或 2)
    pub acpi_version: u32,
    pub _acpi_reserved: u32,

    // ========== SMBIOS 信息 ==========
    /// SMBIOS 入口点地址 (0 表示未找到)
    pub smbios_addr: u64,
    /// SMBIOS 版本
    pub smbios_version: u32,
    pub _smbios_reserved: u32,

    // ========== 存储设备信息 ==========
    /// 磁盘信息数组地址
    pub disk_info_addr: u64,
    /// 检测到的磁盘数量
    pub disk_count: u32,
    /// 启动设备索引 (-1 表示未知)
    pub boot_disk_index: i32,

    // ========== UEFI 运行时服务 ==========
    /// UEFI 运行时服务表地址 (ExitBootServices 后仍可用)
    pub uefi_runtime_services: u64,

    // ========== 内核信息 ==========
    /// 内核加载的物理地址
    pub kernel_phys_addr: u64,
    /// 内核运行的虚拟地址
    pub kernel_virt_addr: u64,
    /// 内核大小（字节）
    pub kernel_size: u64,

    // ========== 页表信息 ==========
    /// PML4 页表物理地址
    pub pml4_phys_addr: u64,
    /// 直接映射区偏移
    pub direct_map_offset: u64,
    /// 运行时内核虚拟地址布局
    pub kernel_layout: KernelVaLayout,

    // ========== 命令行 ==========
    /// 命令行字符串地址（虚拟地址，通过直接映射）
    pub cmdline_addr: u64,
    /// 命令行长度
    pub cmdline_len: u32,
    pub _cmdline_reserved: u32,
    /// 根页表物理地址（v4+，4-level 为 PML4，5-level 为 PML5）
    pub root_table_phys_addr: u64,
}

/// BootInfo 魔数: "JAN_OS\0\0" 的 ASCII 值
pub const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000;
/// BootInfo 版本
pub const BOOTINFO_VERSION: u32 = 4;
/// 内核加载的物理地址
pub const KERNEL_PHYS_ADDR: u64 = 0x100000;
/// 内核运行的虚拟地址（高半部分）
pub const KERNEL_VIRT_ADDR: u64 = 0xFFFF_8000_0010_0000;
/// 直接映射区偏移（物理地址 + 此偏移 = 虚拟地址）
pub const DIRECT_MAP_OFFSET: u64 = crate::cfg::DIRECT_MAP_OFFSET;
/// 最大磁盘数
pub const MAX_DISKS: usize = 32;
/// 最大内存区域数
pub const MAX_MEMORY_REGIONS: usize = 256;
/// 页表缓冲区页数（1 MiB）
pub const PAGE_TABLE_BUFFER_PAGES: usize = 256;
/// 内核启动栈页数（1 MiB）
///
/// `init_kernel` 早期会有较大栈帧（含 stack probe），需要明显大于 512 KiB。
pub const KERNEL_STACK_PAGES: usize = 256;
