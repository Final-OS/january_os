# LRU Cache API

`LruCache` 提供 O(1) 的缓存命中和淘汰管理。

## 关键能力

- `put/get/peek`
- 自动淘汰最久未使用项
- 迭代访问缓存内容

## 示例

```rust
use crate::libs::lru::LruCache;

let mut cache = LruCache::new(128);
cache.put(1, "v");
let _ = cache.get(&1);
```

## 返回

- [数据结构概览](./overview.md)
