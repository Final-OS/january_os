# 内存初始化

本文档详细讲解内存管理子系统的初始化流程。

## 初始化流程

```rust
// kernel/src/main.rs 内存初始化顺序

// 1. Memblock - 早期引导分配器
mm::init_memblock(&regions, kernel_start, kernel_end)?;

// 2. Buddy System - 伙伴系统
mm::init_buddy_system(&regions, max_pfn, direct_map)?;

// 3. SLUB - 小对象分配器
mm::init_slub()?;
mm::finish_mm_init();

// 4. 堆
mm::init_heap(heap_virt, heap_size);

// 5. PCP
mm::init_pcp(batch_size);

// 6. VMA
mm::init_vma();

// 7. NUMA
mm::init_uma();
```

## 1. Memblock 初始化

**文件**: `kernel/src/mm/init.rs`

```rust
pub fn init_memblock(
    regions: &[MemoryRegionInfo],
    kernel_start: u64,
    kernel_end: u64
) -> Result<(), MmError>
```

**功能**:
- 解析 UEFI 内存映射
- 添加可用内存区域
- 保留内核区域
- 设置分配方向（从下往上）

**内存区域处理**:
```rust
for region in regions {
    if region.is_usable {
        memblock_add(region.phys_start, region.page_count * 4096);
    } else {
        memblock_reserve(region.phys_start, region.page_count * 4096);
    }
}

// 保留内核区域
memblock_reserve(kernel_start, kernel_end - kernel_start);
```

## 2. Buddy System 初始化

```rust
pub fn init_buddy_system(
    regions: &[MemoryRegionInfo],
    max_pfn: u64,
    direct_map_offset: u64
) -> Result<(), MmError>
```

**初始化步骤**:
1. 初始化 `struct page` 数组
2. 为每个 Zone 初始化 buddy 系统
3. 设置水位线
4. 初始化空闲列表

**Zone 初始化**:
```rust
// 初始化 ZONE_DMA
init_zone_buddy(ZoneType::DMA, 0, dma_limit / 4096)?;

// 初始化 ZONE_DMA32
init_zone_buddy(ZoneType::DMA32, dma_limit / 4096, dma32_limit / 4096)?;

// 初始化 ZONE_NORMAL
init_zone_buddy(ZoneType::Normal, dma32_limit / 4096, max_pfn)?;
```

**struct page 初始化**:
```rust
pub fn init_vmemmap(start_pfn: u64, end_pfn: u64) {
    for pfn in start_pfn..end_pfn {
        let page = pfn_to_page(pfn);
        page.flags.set(PageFlags::RESERVED);
        page._refcount.store(1, Ordering::Relaxed);
    }
}
```

## 3. SLUB 初始化

```rust
pub fn init_slub() -> Result<(), SlubError>
pub fn finish_mm_init()
```

**步骤**:
1. 创建预定义大小的缓存
2. 初始化 Per-CPU slab
3. 设置默认标志

```rust
// 创建大小缓存
const SLAB_SIZES: &[usize] = &[8, 16, 24, 32, ..., 8192];

for &size in SLAB_SIZES {
    KmemCache::new(
        &format!("kmalloc-{}", size),
        size,
        8
    );
}
```

## 4. 堆初始化

```rust
pub fn init_heap(start: usize, size: usize)
```

**堆布局**:
```
堆起始地址
    │
    ├─ Block 1 (已用)
    ├─ Block 2 (空闲)
    ├─ Block 3 (已用)
    └─ ...
```

**Block 结构**:
```rust
struct Block {
    size: usize,
    used: bool,
}
```

## 5. PCP 初始化

```rust
pub fn init_pcp(batch_size: usize)
```

**Per-CPU 缓存**:
- 每个 CPU 独立的页缓存
- 减少锁竞争
- 批量操作提高效率

## 6. VMA 初始化

```rust
pub fn init_vma()
```

**初始地址空间**:
```rust
let mm = get_init_mm();

// 栈 VMA
let stack_vma = Vma::new(
    VirtAddr::new(0x7ffffffff000 - 0x1000),
    0x1000,
    VmFlags::READ | VmFlags::WRITE | VmFlags::GROWSDOWN
);
mm.mmap(&stack_vma);

// 堆 VMA
let brk_vma = Vma::new(
    VirtAddr::new(0x40000000),
    0x1000,
    VmFlags::READ | VmFlags::WRITE | VmFlags::GROWSUP
);
mm.mmap(&brk_vma);
```

## 7. NUMA 初始化

```rust
pub fn init_uma()   // UMA 模式
pub fn init_numa(srat: &Srat) -> Result<(), NumaError>  // NUMA 模式
```

**UMA (统一内存访问)**:
- 单节点
- 所有 CPU 访问内存延迟相同

**NUMA (非统一内存访问)**:
- 多节点
- 需要从 SRAT 表解析拓扑

## 初始化时序图

```
UEFI ExitBootServices
        │
        ▼
  清零 BSS 段
        │
        ▼
  ┌───────────────────────────────────────────────┐
  │  Memblock 初始化                              │
  │  - 解析内存映射                               │
  │  - 添加可用区域                               │
  │  - 保留内核区域                               │
  └───────────────────────────────────────────────┘
        │
        ▼
  ┌───────────────────────────────────────────────┐
  │  struct page 初始化                           │
  │  - 设置页描述符数组                           │
  │  - 计算最大 PFN                              │
  └───────────────────────────────────────────────┘
        │
        ▼
  ┌───────────────────────────────────────────────┐
  │  Buddy System 初始化                          │
  │  - 初始化各 Zone                              │
  │  - 设置空闲列表                               │
  │  - 设置水位线                                 │
  └───────────────────────────────────────────────┘
        │
        ▼
  ┌───────────────────────────────────────────────┐
  │  SLUB 初始化                                   │
  │  - 创建大小缓存                               │
  │  - 初始化 slab                                │
  └───────────────────────────────────────────────┘
        │
        ▼
  ┌───────────────────────────────────────────────┐
  │  其他组件初始化                               │
  │  - 堆, PCP, VMA, NUMA                        │
  └───────────────────────────────────────────────┘
        │
        ▼
  内存管理就绪
```

## 相关文档

- [API: memblock](../api/mm/memblock.md)
- [API: buddy](../api/mm/buddy.md)
- [API: slub](../api/mm/slub.md)
- [引导流程](./boot.md)
