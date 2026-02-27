# B-Tree API

`BTree` 提供缓存友好的有序映射实现，适合中大型数据集。

## 关键能力

- 有序插入/删除/查找
- 范围遍历
- 头尾元素访问

## 示例

```rust
use crate::libs::btree::BTree;

let mut tree = BTree::new();
tree.insert(1, "one");
let _ = tree.get(&1);
```

## 返回

- [数据结构概览](./overview.md)
