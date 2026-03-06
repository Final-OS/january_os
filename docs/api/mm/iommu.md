# iommu - IOMMU 和 DMA

IOMMU/DMA 运行时已重组到 `kernel/src/mm/dma/`，对外仍通过 `mm` façade 与 `mm::iommu` 兼容别名暴露。

## 当前目录边界

- `kernel/src/mm/dma/mod.rs`：IOMMU 管理器、DMA coherent guard、统一导出
- `kernel/src/mm/dma/swiotlb.rs`：软件 bounce buffer
- `kernel/src/mm/dma/vtd.rs`：Intel VT-d 后端

## 类型

```rust
pub enum IommuType {
    None,
    IntelVtd,
    AmdVi,
    Swiotlb,
}

pub enum TranslationMode {
    Passthrough,
    Translate,
}

pub enum DmaDirection {
    Bidirectional,
    ToDevice,
    FromDevice,
    None,
}

pub struct DmaAddr(pub u64);
```

## API

```rust
pub fn init_iommu();
pub fn iommu_stats() -> IommuStats;
pub fn dma_alloc_coherent(size: usize, gfp_flags: GfpFlags) -> Option<(*mut u8, DmaAddr)>;
pub fn dma_free_coherent(virt: *mut u8, dma: DmaAddr, size: usize);
```

## 当前边界

- `mm::iommu`：兼容入口与 façade 级访问路径
- `mm::dma`：新主目录，负责 IOMMU/SWIOTLB/guard 实现
- `mm::alloc` / `mm::phys`：为 DMA 分配与回收提供页与对象支撑

## 配置

```toml
[iommu]
mode = "auto"
translation = "passthrough"
swiotlb_size = 67108864
```

## 相关文档

- [mmap](./mmap.md)
- [实现: IOMMU](../../implementation/iommu.md)
