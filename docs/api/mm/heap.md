# heap - 早期堆

简单的早期堆分配器，用于内核初始化阶段。

## API

### 堆操作

```rust
pub fn init_heap(start: usize, size: usize)
pub fn heap_stats() -> HeapStats

pub struct HeapStats {
    pub total_size: usize,
    pub used_size: usize,
    pub free_blocks: usize,
}
```

### 分配/释放

```rust
// 堆分配（内部使用）
#[lang = "global_alloc"]
fn alloc(layout: Layout) -> *mut u8

#[lang = "dealloc"]
fn dealloc(ptr: *mut u8, layout: Layout)
```

## 实现原理

**Block 结构**:
```rust
struct Block {
    size: usize,
    used: bool,
    next: Option<&'static mut Block>,
}
```

**堆布局**:
```
堆起始地址
    │
    ├─ Block 1 (已用, size: 256)
    ├─ Block 2 (空闲, size: 512)
    ├─ Block 3 (已用, size: 128)
    └─ ...
```

### 分配策略

```rust
fn alloc_heap(size: usize, align: usize) -> *mut u8 {
    // 首次适配算法
    let mut current = HEAP_START;
    while current < HEAP_END {
        let block = unsafe { &mut *(current as *mut Block) };

        if !block.used && block.size >= size {
            // 找到合适的块
            if block.size >= size + BLOCK_MIN_SIZE {
                // 分割块
                split_block(block, size);
            }
            block.used = true;
            return current as *mut u8 + BLOCK_HEADER_SIZE;
        }

        current += BLOCK_HEADER_SIZE + block.size;
    }

    null_mut()
}
```

### 释放策略

```rust
fn dealloc_heap(ptr: *mut u8, size: usize) {
    let block = unsafe {
        &mut *(ptr as *mut Block).sub(1)
    };

    block.used = false;
    coalesce_with_neighbors(block);
}
```

## 堆配置

```toml
# os_cfg.toml
[kernel]
heap_init_size = 16777216   # 16 MB
```

## 使用场景

### 初始化堆

```rust
use kernel::mm::init_heap;

// 分配堆空间
let heap_page = alloc_pages(8, GFP_KERNEL).unwrap();
let heap_virt = direct_map + page_to_pfn(heap_page) * 4096;
let heap_size = 256 * 4096;  // 1 MB

init_heap(heap_virt as usize, heap_size);
```

### 堆大小

```rust
pub const HEAP_SIZE: usize = 16777216; // 16 MB
```

## 注意事项

1. **早期使用**：仅在内存管理完全初始化前使用
2. **性能**：首次适配算法，性能一般
3. **碎片**：可能产生外碎片
4. **替代**：后期使用 SLUB 替代

## 相关文档

- [slub](./slub.md)
- [buddy](./buddy.md)
- [内存初始化](../../implementation/memory-init.md)
