# iommu - IOMMU 和 DMA

IOMMU (Input/Output Memory Management Unit) 管理设备 DMA 访问，提供地址翻译和设备隔离。

## 概述

IOMMU 允许设备使用虚拟 DMA 地址，隔离设备内存访问，支持 64 位 DMA。

## 类型

```rust
pub enum IommuType {
    None,       // 无 IOMMU
    IntelVtd,   // Intel VT-d
    AmdVi,      // AMD-Vi
    Swiotlb,    // SWIOTLB bounce buffer
}

pub enum TranslationMode {
    Passthrough, // 1:1 映射
    Translate,   // 完整地址翻译
}
```

## API

### 初始化

```rust
pub fn init_iommu()
pub fn iommu_initialized() -> bool
pub fn iommu_enabled() -> bool
```

**示例**：
```rust
use kernel::mm::iommu::{init_iommu, iommu_enabled};

init_iommu();

if iommu_enabled() {
    kprintln!("IOMMU enabled");
}
```

### DMA 一致性分配

```rust
pub fn dma_alloc_coherent(
    size: usize,
    gfp_flags: GfpFlags
) -> Option<(VirtAddr, DmaAddr)>

pub fn dma_free_coherent(
    virt: VirtAddr,
    dma: DmaAddr,
    size: usize
)
```

分配一致性 DMA 内存，CPU 和设备同时访问。

**示例**：
```rust
use kernel::mm::iommu::{dma_alloc_coherent, dma_free_coherent};

// 分配 4KB 一致性内存
if let Some((virt, dma)) = dma_alloc_coherent(4096, GFP_KERNEL) {
    // CPU 写入
    unsafe {
        core::ptr::write_volatile(virt.as_mut_ptr::<u32>(), 0xDEADBEEF);
    }

    // 告诉设备 DMA 地址
    device.start_dma(dma.as_u64());

    // 等待完成...
    while !device.done() {
        core::hint::spin_loop();
    }

    // 释放
    dma_free_coherent(virt, dma, 4096);
}
```

### DMA 流式映射

```rust
pub fn dma_map_single(
    addr: VirtAddr,
    size: usize,
    dir: DmaDirection
) -> Option<DmaAddr>

pub fn dma_unmap_single(
    addr: VirtAddr,
    dma: DmaAddr,
    size: usize
)
```

映射单个缓冲区用于 DMA。

**示例**：
```rust
use kernel::mm::{kmalloc, kfree};
use kernel::mm::iommu::{dma_map_single, dma_unmap_single, DmaDirection};

// 分配缓冲区
let buf = kmalloc(4096, GFP_KERNEL)?;
unsafe {
    core::ptr::write_bytes(buf, 0xDE, 4096);
}

// 映射给设备
let dma = dma_map_single(VirtAddr::new(buf as u64), 4096, DmaDirection::ToDevice)?;

// 同步到设备
dma_sync_single_for_device(dma, 4096);

// 设备访问...
device.start_dma(dma.as_u64());

// 同步回 CPU
dma_sync_single_for_cpu(dma, 4096);

// 读取数据
let data = unsafe { *(buf as *const u32) };

// 解除映射
dma_unmap_single(VirtAddr::new(buf as u64), dma, 4096);
kfree(buf);
```

### DMA 同步

```rust
pub fn dma_sync_single_for_cpu(dma: DmaAddr, size: usize)
pub fn dma_sync_single_for_device(dma: DmaAddr, size: usize)
```

**示例**：
```rust
// CPU 修改后，同步到设备
unsafe {
    core::ptr::write_volatile(ptr, new_value);
}
dma_sync_single_for_device(dma, size);
device.start_transfer(dma.as_u64());

// 设备完成后，同步回 CPU
device.wait_done();
dma_sync_single_for_cpu(dma, size);
let value = unsafe { ptr.read_volatile() };
```

### DMA 方向

```rust
pub enum DmaDirection {
    ToDevice,      // CPU → 设备
    FromDevice,    // 设备 → CPU
    BiDirection,   // 双向
    None,          // 不确定
}
```

### 统计

```rust
pub fn iommu_stats() -> IommuStats

pub struct IommuStats {
    pub enabled: bool,
    pub iommu_type: IommuType,
    pub translation_mode: TranslationMode,
    pub nr_units: usize,
    pub mapped_pages: u64,
}
```

**示例**：
```rust
use kernel::mm::iommu::iommu_stats;

let stats = iommu_stats();
kprintln!("IOMMU: enabled={}, type={:?}, pages={}",
    stats.enabled,
    stats.iommu_type,
    stats.mapped_pages);
```

## 配置

```toml
# os_cfg.toml
[iommu]
mode = "auto"          # off / on / auto
translation = "passthrough"  # passthrough / translate
swiotlb_size = 67108864  # 64 MB
```

### Mode

| 模式 | 说明 |
|------|------|
| `off` | 禁用 IOMMU，使用 SWIOTLB |
| `on` | 强制启用 IOMMU |
| `auto` | 自动检测硬件 |

### Translation

| 模式 | 说明 |
|------|------|
| `passthrough` | 1:1 映射，低开销 |
| `translate` | 完整地址翻译，更安全 |

## SWIOTLB

当硬件 IOMMU 不可用时，使用软件 bounce buffer：

```rust
pub const SWIOTLB_SIZE: usize = 64 * 1024 * 1024; // 64 MB
```

当前实现中，SWIOTLB bounce buffer 通过 Buddy `GFP_DMA32` 分配低端物理内存。

**工作原理**：
```
设备要访问的物理地址超出 DMA 能力范围
    │
    ▼
分配 SWIOTLB buffer (DMA 能力范围内)
    │
    ▼
复制数据: 原地址 → SWIOTLB
    │
    ▼
告诉设备 SWIOTLB 地址
    │
    ▼
设备访问 SWIOTLB
    │
    ▼
复制数据: SWIOTLB → 原地址
```

## Intel VT-d

**DMAR (DMA Remapping) 表解析**：

```rust
// kernel/src/drivers/acpi/dmar.rs

pub struct Dmar {
    pub host_address_width: u8,
    pub flags: DmarFlags,
    pub drhd_units: Vec<DrhdUnit>,
}

pub struct DrhdUnit {
    pub register_address: u64,
    pub segment_number: u16,
    pub devices: Vec<DmarScope>,
}
```

## 使用场景

### 网卡 DMA

```rust
// 分配接收描述符环
let ring_size = 256 * core::mem::size_of::<RxDesc>();
let (ring_virt, ring_dma) = dma_alloc_coherent(ring_size, GFP_KERNEL)?;

// 告诉网卡描述符环地址
nic.set_rx_descriptor_ring(ring_dma.as_u64());

// 分配接收缓冲区
for i in 0..256 {
    let (buf_virt, buf_dma) = dma_alloc_coherent(2048, GFP_KERNEL)?;
    rx_buffers[i] = (buf_virt, buf_dma);

    // 设置描述符
    rx_desc[i].address = buf_dma.as_u64();
    rx_desc[i].status = 0;
}
```

### 块设备 DMA

```rust
// 分散/聚集列表
struct ScatterGather {
    address: DmaAddr,
    length: u32,
}

let mut sg = [
    ScatterGather {
        address: dma_map_single(buf1, 4096, DmaDirection::ToDevice)?,
        length: 4096,
    },
    ScatterGather {
        address: dma_map_single(buf2, 4096, DmaDirection::ToDevice)?,
        length: 4096,
    },
];

// 启动 DMA
block_device.start_dma(&sg);
```

## 注意事项

1. **一致性 vs 流式**：一致性内存开销大，应尽量用流式
2. **缓存一致性**：使用正确的同步函数
3. **地址限制**：设备可能有 32/40 位 DMA 限制
4. **性能**：SWIOTLB 有额外拷贝开销

## 相关文档

- [buddy - 伙伴系统](./buddy.md)
- [vmalloc - 虚拟连续分配](./vmalloc.md)
- [ACPI: DMAR](../../api/drivers/acpi.md)
- [实现: IOMMU](../../implementation/iommu.md)
