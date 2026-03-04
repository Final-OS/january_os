# heap - 内核堆与全局分配器

本模块当前包含两层能力：

1. `SimpleHeap`：分段线性堆（主要用于测试与回退路径）。
2. `#[global_allocator]`：Rust `Box/Vec/String/Arc` 的全局分配入口，优先走 `kmalloc/kfree`（SLUB），无法使用时回退 `SimpleHeap`。

## API

```rust
pub unsafe fn init_heap(target_size: usize) -> usize
pub fn heap_stats() -> HeapStats

pub struct HeapStats {
    pub initialized: bool,
    pub segments: usize,
    pub total_size: usize,
    pub used_size: usize,
    pub free_size: usize,
    pub live_allocations: usize,
}
```

测试接口：

```rust
pub unsafe fn heap_alloc_raw(layout: Layout) -> *mut u8
pub unsafe fn heap_dealloc_raw(ptr: *mut u8, layout: Layout)
```

## 当前实现要点

- `init_heap(target_size)` 会按目标容量预热多个段；每段来自 Buddy 页分配。
- `SimpleHeap` 支持在运行时继续增长段（按需申请更多页）。
- 全局分配器优先调用 `kmalloc`，释放调用 `kfree`。
- 为满足 `Layout::align()`，全局分配器在返回地址前写入头部，释放时回收原始 `kmalloc` 指针。
- 当 SLUB 尚未就绪时，全局分配器回退到 `SimpleHeap`。
- `mm status` 会同时打印 `kmalloc` 统计（主路径）和 `heap(fallback)` 统计（回退路径）。

## 与其他子系统关系

- `Buddy`：提供页级物理页来源。
- `SLUB/kmalloc`：通用小对象分配主路径。
- `heap`：回退与测试路径，保留统计能力。
- `vmalloc`：虚拟连续映射，非 `Box/Vec` 默认路径。

## 配置

`os_cfg.toml` 中通过 `kernel.heap_init_size` 控制预热容量（生成到 `KERNEL_HEAP_INIT_SIZE`）。

## 相关文档

- [slub](./slub.md)
- [buddy](./buddy.md)
- [内存初始化](../../implementation/memory-init.md)
