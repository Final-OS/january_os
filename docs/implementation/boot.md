# 引导流程

本文档详细讲解 january_os 从 UEFI 固件到内核运行的完整引导流程。

## 引导阶段概览

```
┌─────────────────────────────────────────────────────────────────┐
│  1. UEFI 固件                                                  │
│     - POST (上电自检)                                           │
│     - 加载 EFI/BOOT/BOOTX64.EFI                                 │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  2. UEFI 引导程序 (boot/x86_64/src/main.rs)                     │
│     - 初始化 GOP (图形输出)                                     │
│     - 加载 kernel.bin                                          │
│     - 解析 ACPI 表                                             │
│     - 扫描存储设备                                             │
│     - 设置页表 (恒等映射 + 高半映射 + 直接映射)                 │
│     - 退出 Boot Services                                       │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  3. 内核入口 (kernel/src/main.rs::_start)                       │
│     - 清零 BSS                                                 │
│     - 验证 BootInfo                                            │
│     - 初始化串口和 Framebuffer                                  │
│     - 内存管理初始化                                           │
│     - GDT/IDT 初始化                                           │
│     - ACPI 解析                                                │
│     - APIC 初始化                                              │
│     - IOMMU 初始化                                             │
│     - 启用中断                                                 │
│     - 进入 Shell                                               │
└─────────────────────────────────────────────────────────────────┘
```

## UEFI 引导程序详解

### 文件: boot/x86_64/src/main.rs

#### 1. 图形初始化 (GOP)

```rust
fn setup_graphics() -> FramebufferInfo {
    // 1. 获取 GOP 句柄
    let gop_handle = boot::get_handle_for_protocol::<GraphicsOutput>()?;

    // 2. 打开 GOP 协议
    let gop = boot::open_protocol::<GraphicsOutput>(handle)?;

    // 3. 获取当前模式
    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();

    // 4. 获取帧缓冲区
    let mut fb = gop.frame_buffer();
    let fb_addr = fb.as_mut_ptr() as u64;

    // 返回帧缓冲区信息
    FramebufferInfo {
        address: fb_addr,
        width: width as u32,
        height: height as u32,
        // ...
    }
}
```

#### 2. 加载内核

```rust
fn load_kernel() -> usize {
    // 1. 打开文件系统
    let fs = boot::open_protocol::<SimpleFileSystem>(handle)?;
    let mut root = fs.open_volume()?;

    // 2. 打开内核文件
    let kernel_file = root.open(
        cstr16!("\\EFI\\january_os\\kernel.bin"),
        FileMode::Read,
        FileAttribute::empty()
    )?;

    // 3. 获取文件大小
    let file_info = kernel_file.get_info(&mut buf)?;
    let kernel_size = file_info.file_size();

    // 4. 分配内存 (固定地址 0x100000)
    let pages = (kernel_size + 4095) / 4096;
    boot::allocate_pages(
        AllocateType::Address(0x100000),
        MemoryType::LOADER_CODE,
        pages
    )?;

    // 5. 读取内核
    let kernel_buffer = unsafe {
        core::slice::from_raw_parts_mut(0x100000 as *mut u8, kernel_size)
    };
    kernel_file.read(kernel_buffer)?;

    kernel_size
}
```

#### 3. 页表设置

```rust
unsafe fn setup_page_tables(kernel_size: u64, max_phys_addr: u64) -> u64 {
    let mut allocator = PageTableAllocator::new(0x30000);

    // 分配 PML4
    let pml4 = allocator.alloc_page();

    // 1. 恒等映射 (低 4GB)
    // PML4[0] -> PDPT -> 4 个 1GB 大页
    let pdpt_identity = allocator.alloc_page();
    *pml4_table.add(0) = pdpt_identity | PTE_PRESENT | PTE_WRITABLE;

    // 2. 内核高半映射
    // PML4[256] -> PDPT -> PD -> PT
    let pml4_index_kernel = 256;
    let pdpt_kernel = allocator.alloc_page();
    *pml4_table.add(pml4_index_kernel) = pdpt_kernel | ...;

    // 3. 直接映射区
    // PML4[272] -> PDPT -> 1GB 大页
    let pml4_index_direct = 272;
    let pdpt_direct = allocator.alloc_page();
    *pml4_table.add(pml4_index_direct) = pdpt_direct | ...;

    pml4
}
```

#### 4. 跳转到内核

```rust
// 在 main() 函数末尾
let boot_info_ptr = BOOTINFO_PHYS_ADDR as *mut BootInfo;
unsafe {
    asm!(
        "cli",                          // 禁用中断
        "mov cr3, {pml4}",              // 切换页表
        "mov rsp, {stack}",             // 设置栈
        "mov rdi, {boot_info}",         // 第一个参数
        "jmp {entry}",                  // 跳转到内核
        pml4 = in(reg) pml4_addr,
        stack = in(reg) 0x80000u64,
        boot_info = in(reg) BOOTINFO_VIRT_ADDR,
        entry = in(reg) 0x100000,  // 内核物理地址 (恒等映射)
        options(noreturn)
    );
}
```

## 内核入口详解

### 文件: kernel/src/main.rs

#### 1. 清零 BSS

```rust
unsafe fn zero_bss() {
    let start = core::ptr::addr_of!(__bss_start) as *mut u8;
    let end = core::ptr::addr_of!(__bss_end) as *mut u8;
    let size = end as usize - start as usize;
    core::ptr::write_bytes(start, 0, size);
}
```

#### 2. 验证 BootInfo

```rust
const BOOTINFO_MAGIC: u64 = 0x4A414E5F4F530000; // "JAN_OS\0\0"

let info = &*boot_info_ptr;
if info.magic != BOOTINFO_MAGIC {
    panic!("Invalid BootInfo magic: {:#x}", info.magic);
}
```

#### 3. 内存管理初始化

```rust
// [1/6] Memory Management
kprintln!("[1/6] Memory Management");

// 1. Memblock
mm::init_memblock(&regions, kernel_start, kernel_end)?;

// 2. Buddy System
mm::init_buddy_system(&regions, max_pfn, direct_map)?;

// 3. SLUB
mm::init_slub()?;
mm::finish_mm_init();

// 4. 堆
if let Some(heap_page) = mm::alloc_pages(8, mm::GFP_KERNEL) {
    let heap_virt = direct_map + mm::page_to_pfn(heap_page) * 4096;
    mm::init_heap(heap_virt as usize, 256 * 4096);
}

// 5. PCP
mm::init_pcp(4);

// 6. VMA
mm::init_vma();

// 7. NUMA
mm::init_uma();
```

#### 4. GDT/IDT 初始化

```rust
// [2/6] CPU Tables (GDT/IDT)
kprintln!("[2/6] CPU Tables (GDT/IDT)");

// 获取当前栈顶
unsafe {
    core::arch::asm!("mov {}, rsp", out(reg) rsp);
    kernel_stack_top = (rsp + 0xFFF) & !0xFFF;
}

// 初始化 GDT
unsafe { interrupt::init_gdt(kernel_stack_top); }

// 分配 IST1 栈
if let Some(ist_page) = mm::alloc_pages(2, mm::GFP_KERNEL) {
    let ist_top = direct_map + mm::page_to_pfn(ist_page) * 4096 + 16 * 1024;
    interrupt::set_interrupt_stack(1, ist_top);
}
```

#### 5. ACPI 解析

```rust
// [3/6] ACPI Tables
kprintln!("[3/6] ACPI Tables");

let mut local_apic_addr: u64 = 0xFEE00000;
let mut ioapic_addr: u64 = 0;

if info.acpi_rsdp_addr != 0 {
    if acpi::init(info.acpi_rsdp_addr).is_ok() {
        // 解析 MADT
        if let Some(madt) = acpi::get_table::<acpi::Madt>(acpi::MADT_SIGNATURE) {
            let madt_info = acpi::parse_madt(madt);
            cpu_count = madt_info.cpu_count;
            local_apic_addr = madt_info.local_apic_address;
            // ...
        }
    }
}
```

#### 6. APIC 初始化

```rust
// [4/6] Interrupt Controller
kprintln!("[4/6] Interrupt Controller");

let int_info = interrupt::InterruptInitInfo {
    kernel_stack_top,
    local_apic_addr,
    ioapic_addr,
    ioapic_gsi_base,
};

unsafe {
    interrupt::init(&int_info)?;
}
```

#### 7. IOMMU 初始化

```rust
// [5/6] IOMMU
kprintln!("[5/6] IOMMU");

mm::init_iommu();
let stats = mm::iommu_stats();

let iommu_type = match stats.iommu_type {
    mm::IommuType::IntelVtd => "Intel VT-d",
    mm::IommuType::AmdVi => "AMD-Vi",
    mm::IommuType::Swiotlb => "SWIOTLB",
    mm::IommuType::None => "None",
};
```

#### 8. 定时器和中断

```rust
// [6/6] Timer & Interrupts
kprintln!("[6/6] Timer & Interrupts");

// 校准定时器
let timer_freq = interrupt::calibrate_timer();

// 启动 100 Hz 定时器
interrupt::init_apic_timer(interrupt::IRQ_TIMER, 100);

// 启用串口接收中断
serial_enable_rx_interrupt();

// 启用中断
interrupt::enable_interrupts();
```

## 地址映射总结

| 映射类型 | 虚拟地址 | 物理地址 | 大小 |
|----------|----------|----------|------|
| 恒等映射 | 0x00000000+ | 0x00000000+ | 4GB |
| 内核高半 | 0xFFFF_8000_0010_0000+ | 0x100000+ | 动态 |
| 直接映射 | 0xFFFF_8800_0000_0000+ | phys + offset | 64TB |
| vmalloc | 0xFFFF_C900_0000_0000+ | 动态 | 64TB |

## 相关文档

- [内存初始化](./memory-init.md)
- [GDT/TSS](./gdt.md)
- [IDT](./idt.md)
- [APIC](./apic.md)
- [ACPI 解析](./acpi.md)
- [IOMMU](./iommu.md)
