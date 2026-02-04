# init - 内存初始化

内存初始化模块协调各内存组件的初始化顺序。

## API

### 初始化阶段

```rust
pub enum MmInitStage {
    Early,           // Memblock 初始化
    Buddy,          // Buddy System 初始化
    Slub,            // SLUB 初始化
    Finished,        // 完成
}

pub fn init_stage() -> MmInitStage
```

### 初始化函数

```rust
pub fn init_memblock(
    regions: &[MemoryRegionInfo],
    kernel_start: u64,
    kernel_end: u64
) -> Result<(), MmError>

pub fn init_buddy_system(
    regions: &[MemoryRegionInfo],
    max_pfn: u64,
    direct_map_offset: u64
) -> Result<(), MmError>

pub fn init_slub() -> Result<(), SlubError>
pub fn finish_mm_init()

pub fn init_heap(start: usize, size: usize)
pub fn init_pcp(batch_size: usize)
pub fn init_vma()
pub fn init_uma()
pub fn init_numa(srat: &Srat) -> Result<(), NumaError>
```

## 初始化顺序

```
1. Memblock 初始化
   │
   ├─> 解析 UEFI 内存映射
   ├─> 添加可用区域
   ├─> 保留内核区域
   └─> 设置分配方向
        │
        ▼
2. struct page 初始化
   │
   ├─> 初始化页描述符数组
   └─> 设置初始标志
        │
        ▼
3. Buddy System 初始化
   │
   ├─> 初始化 ZONE_DMA
   ├─> 初始化 ZONE_DMA32
   └─> 初始化 ZONE_NORMAL
        │
        ▼
4. SLUB 初始化
   │
   ├─> 创建大小缓存
   └─> 初始化 Per-CPU slab
        │
        ▼
5. 堆初始化
   │
   └─> 初始化全局堆
        │
        ▼
6. PCP 初始化
   │
   └─> 初始化 Per-CPU 缓存
        │
        ▼
7. VMA 初始化
   │
   ├─> 创建初始 VMA
   └─> 设置地址空间
        │
        ▼
8. NUMA 初始化
   │
   ├─> 解析 SRAT (如果是 NUMA)
   └─> 设置 NUMA 节点
        │
        ▼
9. IOMMU 初始化
   │
   ├─> 解析 DMAR
   ├─> 初始化 Intel VT-d
   └─> 回退到 SWIOTLB
        │
        ▼
内存管理就绪
```

## MemoryRegionInfo

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryRegionInfo {
    pub phys_start: u64,
    pub page_count: u64,
    pub is_usable: bool,
}
```

**使用**:
```rust
let regions = &[
    MemoryRegionInfo {
        phys_start: 0x10000000,
        page_count: 16384,
        is_usable: true,
    },
    // ...
];

init_memblock(regions, kernel_start, kernel_end)?;
```

## 初始化检查

```rust
pub fn memblock_initialized() -> bool
pub fn buddy_initialized() -> bool
pub fn slub_initialized() -> bool
```

## 使用示例

### 完整初始化

```rust
use kernel::mm::{
    init_memblock, init_buddy_system, init_slub,
    finish_mm_init, init_heap, init_pcp, init_vma,
    init_uma, MemoryRegionInfo,
};

// 内存区域
let regions = parse_uefi_memory_map(uefi_mmap);

// 内核范围
let kernel_start = 0x100000;
let kernel_end = 0x200000;

// 按顺序初始化
init_memblock(&regions, kernel_start, kernel_end)?;

let max_pfn = calculate_max_pfn(&regions);
init_buddy_system(&regions, max_pfn, direct_map)?;

init_slub()?;
finish_mm_init()?;

// 堆
let heap_page = alloc_pages(8, GFP_KERNEL)?;
let heap_virt = direct_map + page_to_pfn(heap_page) * 4096;
init_heap(heap_virt as usize, 256 * 4096);

// PCP
init_pcp(16);

// VMA
init_vma();

// NUMA
init_uma();
```

## 相关文档

- [memblock](./memblock.md)
- [buddy](./buddy.md)
- [slub](./slub.md)
- [实现: 内存初始化](../../implementation/memory-init.md)
