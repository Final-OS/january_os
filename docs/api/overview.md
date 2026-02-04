# API 概览

本文档提供 january_os 内核各模块的 API 参考。

## 模块索引

### 内存管理 (mm)

| 模块 | 说明 | 文件 |
|------|------|------|
| [address](./mm/address.md) | 物理地址和虚拟地址类型 | `kernel/src/mm/address.rs` |
| [memblock](./mm/memblock.md) | 早期引导内存分配器 | `kernel/src/mm/memblock.rs` |
| [page](./mm/page.md) | 页帧描述符 | `kernel/src/mm/page.rs` |
| [zone](./mm/zone.md) | 内存区域管理 | `kernel/src/mm/zone.rs` |
| [buddy](./mm/buddy.md) | 伙伴系统分配器 | `kernel/src/mm/buddy.rs` |
| [slub](./mm/slub.md) | SLUB 小对象分配器 | `kernel/src/mm/slub.rs` |
| [vma](./mm/vma.md) | 虚拟内存区域 | `kernel/src/mm/vma.rs` |
| [vmalloc](./mm/vmalloc.md) | 虚拟连续分配 | `kernel/src/mm/vmalloc.rs` |
| [fault](./mm/fault.md) | 页错误处理 | `kernel/src/mm/fault.rs` |
| [pcp](./mm/pcp.md) | Per-CPU 页缓存 | `kernel/src/mm/pcp.rs` |
| [numa](./mm/numa.md) | NUMA 支持 | `kernel/src/mm/numa.rs` |
| [iommu](./mm/iommu.md) | IOMMU 和 DMA | `kernel/src/mm/iommu.rs` |
| [paging](./mm/paging.md) | 页表操作 | `kernel/src/mm/paging.rs` |
| [heap](./mm/heap.md) | 早期堆 | `kernel/src/mm/heap.rs` |
| [layout](./mm/layout.md) | 内存布局常量 | `kernel/src/mm/layout.rs` |
| [init](./mm/init.md) | 内存初始化 | `kernel/src/mm/init.rs` |

### 中断处理 (interrupt)

| 模块 | 说明 | 文件 |
|------|------|------|
| [interrupt](./interrupt/interrupt.md) | 中断子系统 | `kernel/src/interrupt/mod.rs` |
| [gdt](./interrupt/gdt.md) | 全局描述符表 | `kernel/src/interrupt/gdt.rs` |
| [idt](./interrupt/idt.md) | 中断描述符表 | `kernel/src/interrupt/idt.rs` |
| [handlers](./interrupt/handlers.md) | 异常和中断处理程序 | `kernel/src/interrupt/handlers.rs` |
| [apic](./interrupt/apic.md) | Local/I/O APIC | `kernel/src/interrupt/apic.rs` |
| [pit](./interrupt/pit.md) | 可编程间隔定时器 | `kernel/src/interrupt/pit.rs` |

### 设备驱动 (drivers)

| 模块 | 说明 | 文件 |
|------|------|------|
| [acpi](./drivers/acpi.md) | ACPI 表解析 | `kernel/src/drivers/acpi/` |
| [tty](./drivers/tty.md) | TTY 子系统 | `kernel/src/drivers/tty/` |
| [input](./drivers/input.md) | 输入设备 | `kernel/src/drivers/input/` |

### 同步原语 (sync)

| 原语 | 说明 | 文件 |
|------|------|------|
| [SpinLock](./sync/spinlock.md) | 自旋锁 | `kernel/src/sync/spinlock.rs` |
| [Mutex](./sync/mutex.md) | 互斥锁 | `kernel/src/sync/mutex.rs` |
| [RwLock](./sync/rwlock.md) | 读写锁 | `kernel/src/sync/rwlock.rs` |
| [Semaphore](./sync/semaphore.md) | 信号量 | `kernel/src/sync/semaphore.rs` |
| [Once](./sync/once.md) | 一次性初始化 | `kernel/src/sync/once.rs` |
| [Barrier](./sync/barrier.md) | 屏障 | `kernel/src/sync/barrier.rs` |

### 架构相关 (arch)

| 模块 | 说明 | 文件 |
|------|------|------|
| [x86_64](./arch/x86_64.md) | x86_64 架构支持 | `kernel/src/arch/x86_64/` |

## 常用类型

### 地址类型

```rust
use kernel::mm::{PhysAddr, VirtAddr};

// 物理地址
let phys = PhysAddr::new(0x100000);
let addr = phys.as_u64();

// 虚拟地址
let virt = VirtAddr::new(0xFFFF800000100000);
let addr = virt.as_u64();

// 转换
let virt = PhysAddr::new(0x100000).to_virt(direct_map_offset);
```

### 页类型

```rust
use kernel::mm::{Page, page_to_pfn, pfn_to_page};

// 页帧号 (PFN) 与 Page 转换
let page = pfn_to_page(256);
let pfn = page_to_pfn(page);
```

### GFP 标志

```rust
use kernel::mm::{GFP_KERNEL, GFP_ATOMIC, GFP_DMA, GFP_DMA32};

// 内核分配（可能睡眠）
let page = alloc_pages(2, GFP_KERNEL);

// 原子分配（不能睡眠）
let page = alloc_pages(2, GFP_ATOMIC);

// 从 DMA 区域分配
let page = alloc_pages(2, GFP_DMA);
```

### Zone 类型

```rust
use kernel::mm::{ZoneType, get_zone};

let zone = get_zone(ZoneType::Normal);
let free = zone.nr_free_pages();
```

## 内存分配 API

### 页分配

```rust
use kernel::mm::{alloc_pages, free_pages, alloc_page, free_page};

// 分配 2^order 页
let page = alloc_pages(3, GFP_KERNEL); // 8 页

// 释放
free_pages(page, 3);

// 分配单页
let page = alloc_page(GFP_KERNEL);
free_page(page);
```

### 小对象分配

```rust
use kernel::mm::{kmalloc, kfree, kzalloc};

// 分配
let ptr = kmalloc(64, GFP_KERNEL);
let ptr = kzalloc(64, GFP_KERNEL); // 分配并清零

// 释放
kfree(ptr);
```

### 虚拟连续分配

```rust
use kernel::mm::{vmalloc, vzalloc, vfree};

// 分配虚拟连续内存
let ptr = vmalloc(4096, GFP_KERNEL);
let ptr = vzalloc(4096, GFP_KERNEL); // 分配并清零

// 释放
vfree(ptr);
```

## 中断 API

### 中断控制

```rust
use kernel::interrupt::{
    enable_interrupts,
    disable_interrupts,
    interrupts_enabled,
    without_interrupts,
};

// 启用/禁用中断
disable_interrupts();
enable_interrupts();

// 检查状态
let enabled = interrupts_enabled();

// 执行时禁用中断
without_interrupts(|| {
    // 临界区代码
});
```

### APIC

```rust
use kernel::interrupt::{
    local_apic_id,
    local_apic_eoi,
    calibrate_timer,
    init_apic_timer,
};

// 获取 APIC ID
let id = local_apic_id();

// 发送 EOI
local_apic_eoi();

// 定时器
let bus_freq = calibrate_timer();
init_apic_timer(0x20, 100); // 100 Hz
```

## 同步 API

### SpinLock

```rust
use kernel::sync::SpinLock;

let lock = SpinLock::new(42);

{
    let guard = lock.lock();
    *guard = 43;
} // 自动释放

// 或使用
let value = *lock.lock();
```

### Mutex

```rust
use kernel::sync::Mutex;

let mutex = Mutex::new(42);

{
    let guard = mutex.lock();
    *guard = 43;
}
```

### Once

```rust
use kernel::sync::Once;

static INIT: Once = Once::new();

INIT.call_once(|| {
    // 只执行一次
    expensive_init();
});
```

## 相关文档

- [实现详解](../implementation/overview.md)
- [配置说明](../guide/configuration.md)
