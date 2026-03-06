# init - 内存初始化

内存初始化编排已按组件化宏内核规范重组到 `kernel/src/mm/boot/setup.rs`，并由 `kernel/src/mm/mod.rs` 作为 façade 统一导出。

## 当前目录边界

```text
kernel/src/mm/
├── mod.rs        # façade：生命周期、稳定导出、跨域胶水
├── api/          # layout 等稳定常量与接口
├── runtime/      # init stage / 运行态占位
├── boot/         # 启动期内存导入与 setup
├── phys/         # memblock/page/zone/buddy/numa/pcp
├── virt/         # address/vma/fault/paging/layout_runtime
├── alloc/        # heap/slub/vmalloc
├── dma/          # iommu/swiotlb/vtd
├── syscall/      # mmap/munmap/mprotect/brk ABI
├── diag/         # dump/stats
└── arch/         # 架构后端
```

## API

### 初始化阶段

```rust
pub enum MmInitStage {
    None,
    Memblock,
    Buddy,
    Slub,
    Complete,
}

pub fn init_stage() -> MmInitStage
```

### 初始化函数

```rust
pub unsafe fn init_memblock(
    regions: &[MemoryRegionInfo],
    kernel_start: u64,
    kernel_end: u64,
) -> KernelResult<()>;

pub unsafe fn init_buddy_system(
    regions: &[MemoryRegionInfo],
    max_pfn: u64,
    direct_map_offset: u64,
) -> KernelResult<()>;

pub unsafe fn init_slub() -> KernelResult<()>;
pub fn finish_mm_init() -> KernelResult<()>;

pub fn init_heap(target_bytes: usize) -> usize;
pub fn init_pcp(batch_size: usize);
pub fn init_vma();
pub fn init_uma();
pub fn init_numa(srat: &Srat) -> Result<(), NumaError>;
```

## 初始化顺序

```text
memblock -> struct page/vmemmap -> buddy -> SLUB -> heap/PCP/VMA -> NUMA -> IOMMU -> complete
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

## 当前边界

- `boot/setup.rs`：启动期初始化顺序、阶段推进、保留区与 zone/vmemmap 编排
- `phys/`：memblock、page、zone、buddy、numa、pcp 真实实现
- `alloc/`：SLUB 与堆增长策略
- `virt/`：布局运行态、VMA 初始化与页表协作
- `dma/`：IOMMU/SWIOTLB 最后接入

## 相关文档

- [memblock](./memblock.md)
- [buddy](./buddy.md)
- [slub](./slub.md)
- [实现: 内存初始化](../../implementation/memory-init.md)
