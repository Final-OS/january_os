# RbTree API

`RbTree` 提供有序键值存储，适合需要区间遍历和有序查找的场景。

## 关键能力

- `insert/get/remove`
- 有序迭代
- 范围查询

## 示例

```rust
use crate::libs::rbtree::RbTree;

let mut tree = RbTree::new();
tree.insert(1, "one");
let v = tree.get(&1);
tree.remove(&1);
```

## 返回

- [数据结构概览](./overview.md)
