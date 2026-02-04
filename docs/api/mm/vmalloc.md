# vmalloc - 虚拟连续内存分配

vmalloc 分配虚拟连续但物理可能不连续的内存，适用于需要大块内存但不需要物理连续的场景。

## 概述

vmalloc 区域位于 `0xFFFF_C900_0000_0000`，提供 64TB 的虚拟地址空间。

## API

### 基本分配

```rust
pub fn vmalloc(size: usize, gfp_flags: GfpFlags) -> Option<VirtAddr>
pub fn vzalloc(size: usize, gfp_flags: GfpFlags) -> Option<VirtAddr>
pub fn vfree(addr: VirtAddr)
```

**参数**：
- `size`: 分配大小（字节）
- `gfp_flags`: 分配标志

**返回值**：虚拟地址，失败返回 `None`

**示例**：
```rust
use kernel::mm::{vmalloc, vzalloc, vfree, GFP_KERNEL};

// 分配 1MB
if let Some(addr) = vmalloc(1024 * 1024, GFP_KERNEL) {
    // 使用内存...
    vfree(addr);
}

// 分配并清零
if let Some(addr) = vzalloc(1024 * 1024, GFP_KERNEL) {
    // 内存已清零...
    vfree(addr);
}
```

### I/O 内存映射

```rust
pub fn ioremap(phys_addr: PhysAddr, size: usize) -> Option<VirtAddr>
pub fn iounmap(virt_addr: VirtAddr)
```

映射物理内存到虚拟地址空间，用于访问设备寄存器。

**示例**：
```rust
use kernel::mm::{PhysAddr, ioremap, iounmap};

// 映射设备寄存器
let phys = PhysAddr::new(0xFE000000);
if let Some(virt) = ioremap(phys, 0x1000) {
    // 读写设备寄存器
    unsafe {
        let reg = virt.as_ptr::<u32>().read_volatile();
    }

    // 解除映射
    iounmap(virt);
}
```

### 初始化和状态

```rust
pub fn init_vmalloc()
pub fn vmalloc_initialized() -> bool
pub fn vmalloc_stats() -> VmallocStats
```

**统计信息**：
```rust
pub struct VmallocStats {
    pub total_blocks: usize,
    pub used_blocks: usize,
    pub total_bytes: u64,
    pub used_bytes: u64,
}

// 示例
let stats = vmalloc_stats();
kprintln!("VMalloc: {} / {} bytes",
    stats.used_bytes,
    stats.total_bytes);
```

## 地址范围

```rust
pub const VMALLOC_START: u64 = 0xFFFF_C900_0000_0000;
pub const VMALLOC_END: u64 = 0xFFFF_FFFF_FFFF_F000;
```

## 分配特点

| 特性 | vmalloc | kmalloc |
|------|---------|---------|
| 虚拟连续 | 是 | 是 |
| 物理连续 | 否 | 是 |
| 最大大小 | 大 (受虚拟地址空间限制) | 小 (受物理连续限制) |
| 分配速度 | 较慢 | 较快 |
| 使用场景 | 大块内存、DMA 缓冲区 | 小对象、结构体 |

## 使用场景

### 大缓冲区

```rust
// 分配 10MB 缓冲区
let size = 10 * 1024 * 1024;
if let Some(buf) = vmalloc(size, GFP_KERNEL) {
    // 使用大缓冲区...
    vfree(buf);
}
```

### 模块加载

```rust
// 加载内核模块到 vmalloc 区域
let module_size = module_data.len();
if let Some(addr) = vmalloc(module_size, GFP_KERNEL) {
    unsafe {
        core::ptr::copy_nonoverlapping(
            module_data.as_ptr(),
            addr.as_mut_ptr::<u8>(),
            module_size
        );
    }
    // 执行模块...
    vfree(addr);
}
```

### 设备映射

```rust
// 映射 PCI 设备的 MMIO 空间
fn map_pci_device(bar_addr: PhysAddr, size: usize) -> Option<VirtAddr> {
    ioremap(bar_addr, size)
}

// 使用
if let Some(mmio) = map_pci_device(PhysAddr::new(0xF0000000), 0x10000) {
    // 访问设备寄存器
    unsafe {
        let value = mmio.as_ptr::<u32>().read_volatile();
    }
    iounmap(mmio);
}
```

## 内存布局

```
vmalloc 区域 (64TB)
┌─────────────────────────────────────────────────────────┐
│  0xFFFF_C900_0000_0000                                 │
│                                                         │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐               │
│  │ Block 1 │  │ Block 2 │  │ Block 3 │  ...           │
│  │  1MB    │  │  2MB    │  │  512KB  │               │
│  └─────────┘  └─────────┘  └─────────┘               │
│                                                         │
│  每个块可能由多个不连续的物理页组成                    │
│                                                         │
└─────────────────────────────────────────────────────────┘
  0xFFFF_FFFF_FFFF_F000

Block 1 示例:
虚拟:  0xFFFF_C900_0000_0000 - 0xFFFF_C900_0010_0000 (1MB)
物理:
  0x1000_0000 - 0x1000_1000 (页 0)
  0x2000_0000 - 0x2000_1000 (页 1)
  0x3000_0000 - 0x3000_1000 (页 2)
  ...
```

## 分配流程

```
vmalloc(size, GFP_KERNEL)
    │
    ▼
计算需要多少页
    │
    ▼
逐页分配物理内存
    │
    ├─ 成功 ──► 建立页表映射
    │              │
    │              └─► 返回虚拟地址
    │
    └─ 失败 ──► 释放已分配的页
                  │
                  └─► 返回 None
```

## 释放流程

```
vfree(addr)
    │
    ▼
查找 vmalloc 块
    │
    ▼
取消页表映射
    │
    ▼
逐页释放物理内存
    │
    ▼
释放虚拟地址
```

## 注意事项

1. **不物理连续**：不适合需要物理连续内存的场景（如 DMA）
2. **页表开销**：每个分配需要额外的页表映射
3. **TLB 压力**：大量使用会增加 TLB miss
4. **大小限制**：实际最大分配受物理内存限制

## DMA 考虑

对于 DMA 操作，使用 `dma_alloc_coherent` 而不是 vmalloc：

```rust
// DMA 一致性分配（物理连续）
let (virt, dma) = dma_alloc_coherent(size, GFP_KERNEL);

// 而不是 vmalloc（不物理连续）
let virt = vmalloc(size, GFP_KERNEL); // ❌ 不适合 DMA
```

## 相关文档

- [buddy - 伙伴系统](./buddy.md)
- [slub - 小对象分配器](./slub.md)
- [iommu - DMA 分配](./iommu.md)
- [paging - 页表操作](./paging.md)
