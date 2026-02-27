// ============================================================================
// january_os - GDT (Global Descriptor Table)
//
// x86_64 段描述符表实现
// ============================================================================
//!
//! # GDT 布局
//!
//! ```text
//! Index  Selector  Description
//! ─────────────────────────────────────
//! 0      0x00      Null Descriptor
//! 1      0x08      Kernel Code (64-bit)
//! 2      0x10      Kernel Data
//! 3      0x18      User Code (64-bit)
//! 4      0x20      User Data
//! 5-6    0x28      TSS (128-bit)
//! ```

use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::size_of;

// ============================================================================
// 段选择子
// ============================================================================

/// 内核代码段选择子
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
/// 内核数据段选择子
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
/// 用户代码段选择子 (RPL=3)
pub const USER_CODE_SELECTOR: u16 = 0x18 | 3;
/// 用户数据段选择子 (RPL=3)
pub const USER_DATA_SELECTOR: u16 = 0x20 | 3;
/// TSS 选择子
pub const TSS_SELECTOR: u16 = 0x28;

// ============================================================================
// 段描述符
// ============================================================================

/// 段描述符 (64位)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct SegmentDescriptor {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl SegmentDescriptor {
    /// 创建空描述符
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    /// 创建内核代码段描述符 (64位长模式)
    pub const fn kernel_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            // Present | DPL=0 | Code | Executable | Readable
            access: 0b1001_1010,
            // Granularity | Long mode | Limit high
            granularity: 0b1010_1111,
            base_high: 0,
        }
    }

    /// 创建内核数据段描述符
    pub const fn kernel_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            // Present | DPL=0 | Data | Writable
            access: 0b1001_0010,
            // Granularity | 32-bit | Limit high
            granularity: 0b1100_1111,
            base_high: 0,
        }
    }

    /// 创建用户代码段描述符 (64位长模式)
    pub const fn user_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            // Present | DPL=3 | Code | Executable | Readable
            access: 0b1111_1010,
            // Granularity | Long mode | Limit high
            granularity: 0b1010_1111,
            base_high: 0,
        }
    }

    /// 创建用户数据段描述符
    pub const fn user_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            // Present | DPL=3 | Data | Writable
            access: 0b1111_0010,
            // Granularity | 32-bit | Limit high
            granularity: 0b1100_1111,
            base_high: 0,
        }
    }
}

// ============================================================================
// TSS (Task State Segment)
// ============================================================================

/// TSS 描述符 (128位，占用两个 GDT 槽位)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct TssDescriptor {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssDescriptor {
    /// 创建空 TSS 描述符
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
            base_upper: 0,
            reserved: 0,
        }
    }

    /// 从 TSS 地址创建描述符
    pub fn new(tss_addr: u64, tss_size: u16) -> Self {
        Self {
            limit_low: tss_size,
            base_low: tss_addr as u16,
            base_middle: (tss_addr >> 16) as u8,
            // Present | DPL=0 | TSS Available (0x89)
            access: 0x89,
            granularity: 0x00,
            base_high: (tss_addr >> 24) as u8,
            base_upper: (tss_addr >> 32) as u32,
            reserved: 0,
        }
    }
}

/// 任务状态段 (Task State Segment)
#[repr(C, packed)]
pub struct Tss {
    reserved1: u32,
    /// 特权级 0 的栈指针
    pub rsp0: u64,
    /// 特权级 1 的栈指针
    pub rsp1: u64,
    /// 特权级 2 的栈指针
    pub rsp2: u64,
    reserved2: u64,
    /// 中断栈表 (IST1-IST7)
    pub ist: [u64; 7],
    reserved3: u64,
    reserved4: u16,
    /// I/O 权限位图偏移
    pub iomap_base: u16,
}

impl Tss {
    /// 创建新的 TSS
    pub const fn new() -> Self {
        Self {
            reserved1: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved2: 0,
            ist: [0; 7],
            reserved3: 0,
            reserved4: 0,
            iomap_base: size_of::<Self>() as u16,
        }
    }

    /// 设置内核栈 (用于从用户态切换到内核态)
    pub fn set_kernel_stack(&mut self, stack_top: u64) {
        self.rsp0 = stack_top;
    }

    /// 设置中断栈 (IST, 1-7)
    pub fn set_interrupt_stack(&mut self, ist_index: usize, stack_top: u64) {
        if ist_index > 0 && ist_index <= 7 {
            self.ist[ist_index - 1] = stack_top;
        }
    }
}

// ============================================================================
// GDT 结构
// ============================================================================

/// GDT 条目数量
const GDT_ENTRIES: usize = 7;

/// GDT 表
#[repr(C, align(16))]
pub struct Gdt {
    entries: [u64; GDT_ENTRIES],
}

impl Gdt {
    /// 创建新的 GDT
    pub const fn new() -> Self {
        Self {
            entries: [0; GDT_ENTRIES],
        }
    }

    /// 设置段描述符
    fn set_entry(&mut self, index: usize, desc: SegmentDescriptor) {
        let bytes = unsafe {
            core::mem::transmute::<SegmentDescriptor, u64>(desc)
        };
        self.entries[index] = bytes;
    }

    /// 设置 TSS 描述符 (占用两个槽位)
    fn set_tss(&mut self, index: usize, desc: TssDescriptor) {
        let bytes = unsafe {
            core::mem::transmute::<TssDescriptor, [u64; 2]>(desc)
        };
        self.entries[index] = bytes[0];
        self.entries[index + 1] = bytes[1];
    }

    /// 初始化 GDT
    pub fn init(&mut self, tss: &Tss) {
        // 0: Null descriptor
        self.set_entry(0, SegmentDescriptor::null());
        // 1: Kernel code (0x08)
        self.set_entry(1, SegmentDescriptor::kernel_code());
        // 2: Kernel data (0x10)
        self.set_entry(2, SegmentDescriptor::kernel_data());
        // 3: User code (0x18)
        self.set_entry(3, SegmentDescriptor::user_code());
        // 4: User data (0x20)
        self.set_entry(4, SegmentDescriptor::user_data());
        // 5-6: TSS (0x28)
        let tss_addr = tss as *const Tss as u64;
        let tss_size = (size_of::<Tss>() - 1) as u16;
        self.set_tss(5, TssDescriptor::new(tss_addr, tss_size));
    }

    /// 获取 GDTR 值
    fn gdtr(&self) -> GdtPointer {
        GdtPointer {
            limit: (size_of::<Self>() - 1) as u16,
            base: self.entries.as_ptr() as u64,
        }
    }

    /// 加载 GDT
    /// 
    /// # Safety
    /// 
    /// 调用者必须确保 GDT 已正确初始化
    pub unsafe fn load(&self) {
        let gdtr = self.gdtr();
        unsafe {
            asm!(
                "lgdt [{}]",
                in(reg) &gdtr,
                options(nostack, preserves_flags)
            );
        }
    }
}

/// GDTR 寄存器格式
#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

use crate::config::MAX_CPUS;

// ============================================================================
// 全局实例 (Per-CPU)
// ============================================================================

/// 全局 GDT 数组
struct GdtArray {
    inner: UnsafeCell<[Gdt; MAX_CPUS]>,
}

unsafe impl Sync for GdtArray {}

impl GdtArray {
    const fn new() -> Self {
        const UNINIT: Gdt = Gdt::new();
        Self {
            inner: UnsafeCell::new([UNINIT; MAX_CPUS]),
        }
    }
}

static GDTS: GdtArray = GdtArray::new();

/// 全局 TSS 数组
struct TssArray {
    inner: UnsafeCell<[Tss; MAX_CPUS]>,
}

unsafe impl Sync for TssArray {}

impl TssArray {
    const fn new() -> Self {
        const UNINIT: Tss = Tss::new();
        Self {
            inner: UnsafeCell::new([UNINIT; MAX_CPUS]),
        }
    }
}

static TSSS: TssArray = TssArray::new();

#[inline]
unsafe fn gdt_mut(cpu_id: usize) -> &'static mut Gdt {
    unsafe { &mut (*GDTS.inner.get())[cpu_id] }
}

#[inline]
unsafe fn tss_mut(cpu_id: usize) -> &'static mut Tss {
    unsafe { &mut (*TSSS.inner.get())[cpu_id] }
}

/// 初始化 GDT 和 TSS
/// 
/// # Arguments
/// * `cpu_id` - CPU ID (0 for BSP)
/// * `kernel_stack_top` - 该 CPU 的内核栈顶地址
/// 
/// # Safety
/// 
/// 必须在中断禁用时调用。
/// 每个 CPU 必须使用唯一的 cpu_id。
pub unsafe fn init_gdt(cpu_id: usize, kernel_stack_top: u64) {
    if cpu_id >= MAX_CPUS {
        panic!("CPU ID {} exceeds MAX_CPUS {}", cpu_id, MAX_CPUS);
    }

    unsafe {
        // 设置 TSS
        let tss = tss_mut(cpu_id);
        tss.set_kernel_stack(kernel_stack_top);
        
        // 初始化 GDT
        let gdt = gdt_mut(cpu_id);
        gdt.init(tss);
        
        // 加载 GDT
        gdt.load();
        
        // 重新加载段寄存器
        reload_segments();
        
        // 加载 TSS
        load_tss();
    }
}

/// 重新加载段寄存器
unsafe fn reload_segments() {
    unsafe {
        asm!(
            // 使用 far return 重新加载 CS
            "push {code_sel}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            // 重新加载数据段寄存器
            "mov ax, {data_sel}",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            code_sel = const KERNEL_CODE_SELECTOR as u64,
            data_sel = const KERNEL_DATA_SELECTOR,
            options(nostack)
        );
    }
}

/// 加载 TSS
unsafe fn load_tss() {
    unsafe {
        asm!(
            "ltr {sel:x}",
            sel = in(reg) TSS_SELECTOR,
            options(nostack, preserves_flags)
        );
    }
}

/// 获取 TSS 可变引用
/// 
/// # Safety
/// 
/// 调用者必须确保没有并发访问
pub unsafe fn get_tss_mut(cpu_id: usize) -> &'static mut Tss {
    if cpu_id >= MAX_CPUS {
        panic!("CPU ID {} exceeds MAX_CPUS {}", cpu_id, MAX_CPUS);
    }
    unsafe { tss_mut(cpu_id) }
}

/// 设置中断栈
/// 
/// # Arguments
/// * `cpu_id` - CPU ID
/// * `ist_index` - IST 索引 (1-7)
/// * `stack_top` - 栈顶地址
pub fn set_interrupt_stack(cpu_id: usize, ist_index: usize, stack_top: u64) {
    unsafe {
        let tss = get_tss_mut(cpu_id);
        tss.set_interrupt_stack(ist_index, stack_top);
    }
}
