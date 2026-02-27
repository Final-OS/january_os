# Maple Tree API

`MapleTree` 是面向区间管理的数据结构，用于 VMA 等区间场景。

## 关键能力

- 区间插入/删除/查找
- 区间重叠判断
- 间隙搜索

## 示例

```rust
use crate::libs::mptree::MapleTree;

let mut tree = MapleTree::new();
tree.insert(0..4096, "region");
let _ = tree.get(1024);
```

## 返回

- [数据结构概览](./overview.md)
