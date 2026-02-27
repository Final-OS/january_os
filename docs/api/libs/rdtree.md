# Radix Tree API

`RadixTree` 用于稀疏整数索引场景，常见于页缓存和 ID 映射。

## 关键能力

- 稀疏键插入/查找
- 范围遍历
- 间隙搜索

## 示例

```rust
use crate::libs::rdtree::RadixTree;

let mut tree = RadixTree::new();
tree.insert(100, "value");
let _ = tree.get(100);
```

## 返回

- [数据结构概览](./overview.md)
