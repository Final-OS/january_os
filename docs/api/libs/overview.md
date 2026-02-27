# Data Structures API

内核数据结构库提供高性能、线程安全的数据结构实现。

---

## 概述

january_os 内核实现了以下数据结构：

| 数据结构 | 用途 | 性能 | 状态 |
|---------|------|------|------|
| [RbTree](#rbtree) | 有序键值对 | O(log n) | ✅ 完成 |
| [LRU Cache](#lru-cache) | 缓存管理 | O(1) | ✅ 完成 |
| [RCU](#rcu) | 无锁同步 | O(1) 读取 | ✅ 完成 |
| [Radix Tree](#radix-tree) | 稀疏数组 | O(log₆₄ n) | ✅ 完成 |
| [B-Tree](#b-tree) | 通用有序树 | O(log₁₆ n) | ✅ 完成 |
| [Maple Tree](#maple-tree) | 区间树 | O(log₁₆ n) | ✅ 完成 |

---

## RbTree

红黑树，提供有序键值对存储。

### 特性

- **时间复杂度**: O(log n) 插入/删除/查找
- **空间复杂度**: O(n)
- **线程安全**: 需要外部同步
- **用途**: VMA 管理、调度器、文件系统

### API

```rust
use crate::libs::rbtree::RbTree;

// 创建
let mut tree = RbTree::new();

// 插入
tree.insert(1, "one");
tree.insert(2, "two");

// 查找
assert_eq!(tree.get(&1), Some(&"one"));

// 删除
tree.remove(&1);

// 迭代
for (key, value) in tree.iter() {
    println!("{}: {}", key, value);
}
```

### 详细文档

参见 [RbTree API](./rbtree.md)

---

## LRU Cache

最近最少使用缓存，O(1) 所有操作。

### 特性

- **时间复杂度**: O(1) get/put/remove
- **空间复杂度**: O(n)
- **实现**: 双向链表 + HashMap
- **用途**: 页缓存、文件缓存、对象缓存

### API

```rust
use crate::libs::lru::LruCache;

// 创建容量为 3 的缓存
let mut cache = LruCache::new(3);

// 插入
cache.put(1, "a");
cache.put(2, "b");
cache.put(3, "c");

// 获取（更新 LRU 顺序）
assert_eq!(cache.get(&1), Some(&"a"));

// 插入第 4 个元素，淘汰 LRU
cache.put(4, "d");
assert_eq!(cache.get(&2), None); // 被淘汰

// 查看（不更新顺序）
assert_eq!(cache.peek(&3), Some(&"c"));

// 弹出 LRU
let lru = cache.pop_lru();
```

### 高级功能

```rust
// get_or_insert
let val = cache.get_or_insert(5, "e");

// 调整容量
let evicted = cache.resize(5);

// 迭代（MRU 到 LRU）
for (key, value) in cache.iter() {
    println!("{}: {}", key, value);
}
```

### 详细文档

参见 [LRU Cache API](./lru.md)

---

## RCU

Read-Copy-Update，无锁读取同步机制。

### 特性

- **读取**: O(1)，无锁
- **更新**: O(1)，需要同步
- **宽限期**: 自动管理
- **用途**: 高并发读取场景

### API

```rust
use crate::libs::rcu::Rcu;

// 创建
let rcu = Rcu::new(42);

// 读取（无锁）
{
    let guard = rcu.read();
    println!("Value: {}", *guard);
}

// 更新（同步）
let old = rcu.update(100);

// 基于当前值更新
let old = rcu.update_with(|val| *val * 2);

// 异步更新
rcu.update_async(200);

// 等待宽限期
rcu.synchronize_rcu();

// 延迟回调
rcu.call_rcu(|| {
    println!("Callback executed");
});
```

### 详细文档

参见 [RCU API](./rcu.md)

---

## Radix Tree

64-way 多级基数树，适合稀疏数组。

### 特性

- **时间复杂度**: O(log₆₄ n)
- **分支因子**: 64
- **用途**: 页缓存、IDR、文件映射

### API

```rust
use crate::libs::rdtree::RadixTree;

// 创建
let mut tree = RadixTree::new();

// 插入
tree.insert(0, "zero");
tree.insert(100, "hundred");
tree.insert(1000, "thousand");

// 查找
assert_eq!(tree.get(100), Some(&"hundred"));

// 范围查询
for (index, value) in tree.range(50..150) {
    println!("{}: {}", index, value);
}

// 间隙搜索
let gap = tree.find_first_gap_from(0);
assert_eq!(gap, Some(1));

// 迭代
for (index, value) in tree.iter() {
    println!("{}: {}", index, value);
}
```

### 详细文档

参见 [Radix Tree API](./rdtree.md)

---

## B-Tree

通用 B-Tree，分支因子 16，缓存优化。

### 特性

- **时间复杂度**: O(log₁₆ n)
- **分支因子**: 16
- **树高**: 比红黑树低 4 倍
- **用途**: 通用有序映射

### API

```rust
use crate::libs::btree::BTree;

// 创建
let mut tree = BTree::new();

// 插入
tree.insert(1, "one");
tree.insert(2, "two");
tree.insert(3, "three");

// 查找
assert_eq!(tree.get(&2), Some(&"two"));

// 包含键
assert!(tree.contains_key(&1));

// 获取可变引用
if let Some(val) = tree.get_mut(&1) {
    *val = "ONE";
}

// 删除
tree.remove(&1);

// 范围查询
for (key, value) in tree.range(1..=10) {
    println!("{}: {}", key, value);
}

// 迭代
for (key, value) in tree.iter() {
    println!("{}: {}", key, value);
}
```

### 高级功能

```rust
// 第一个/最后一个
let first = tree.first();
let last = tree.last();

// 弹出第一个/最后一个
let first = tree.pop_first();
let last = tree.pop_last();

// 保留满足条件的元素
tree.retain(|k, v| *k > 10);

// 清空
tree.clear();
```

### 详细文档

参见 [B-Tree API](./btree.md)

---

## Maple Tree

区间树，基于 B-Tree 实现，用于 VMA 管理。

### 特性

- **时间复杂度**: O(log₁₆ n)
- **区间查询**: 高效
- **间隙搜索**: 支持
- **用途**: VMA 管理、资源分配

### API

```rust
use crate::libs::mptree::MapleTree;

// 创建
let mut tree = MapleTree::new();

// 插入区间
tree.insert(0..100, "region1");
tree.insert(200..300, "region2");

// 查找区间
if let Some(value) = tree.get(50) {
    println!("Found: {}", value);
}

// 查找间隙
let gap = tree.find_gap(0, 1000, 100);
if let Some((start, end)) = gap {
    println!("Gap: {}..{}", start, end);
}

// 反向查找间隙
let gap = tree.find_gap_reverse(1000, 0, 100);

// 删除区间
tree.remove(0..100);

// 迭代
for (range, value) in tree.iter() {
    println!("{:?}: {}", range, value);
}
```

### 详细文档

参见 [Maple Tree API](./mptree.md)

---

## 性能对比

### 查找性能

| 数据结构 | 1K 元素 | 10K 元素 | 100K 元素 | 1M 元素 |
|---------|---------|----------|-----------|---------|
| RbTree | ~10 次 | ~13 次 | ~17 次 | ~20 次 |
| B-Tree | ~3 次 | ~4 次 | ~5 次 | ~6 次 |
| Radix Tree | ~2 次 | ~3 次 | ~4 次 | ~4 次 |

### 内存占用

| 数据结构 | 每元素开销 | 1K 元素 | 10K 元素 |
|---------|-----------|---------|----------|
| RbTree | ~32 B | ~32 KB | ~320 KB |
| B-Tree | ~18 B | ~18 KB | ~180 KB |
| LRU Cache | ~48 B | ~48 KB | ~480 KB |

---

## 选择指南

### 何时使用 RbTree

- ✅ 需要有序键值对
- ✅ 内存受限
- ✅ 小规模数据（< 10K）
- ❌ 大规模数据

### 何时使用 B-Tree

- ✅ 大规模有序数据（> 10K）
- ✅ 需要范围查询
- ✅ 缓存友好
- ❌ 内存受限

### 何时使用 Radix Tree

- ✅ 稀疏整数索引
- ✅ 页缓存
- ✅ IDR 分配
- ❌ 非整数键

### 何时使用 LRU Cache

- ✅ 需要缓存管理
- ✅ 需要淘汰策略
- ✅ O(1) 操作
- ❌ 不需要淘汰

### 何时使用 RCU

- ✅ 高并发读取
- ✅ 读多写少
- ✅ 需要无锁读取
- ❌ 写多读少

### 何时使用 Maple Tree

- ✅ 区间管理
- ✅ VMA 管理
- ✅ 需要间隙搜索
- ❌ 点查询

---

## 线程安全

### 内部同步

- **RCU**: 读取无锁，写入需要同步
- **其他**: 需要外部同步（Mutex/RwLock）

### 使用示例

```rust
use crate::sync::Mutex;
use crate::libs::btree::BTree;

// 使用 Mutex 保护
let tree = Mutex::new(BTree::new());

// 插入
{
    let mut t = tree.lock();
    t.insert(1, "one");
}

// 查找
{
    let t = tree.lock();
    let val = t.get(&1);
}
```

---

## 测试覆盖

所有数据结构都有完整的测试覆盖：

- ✅ 基础操作测试
- ✅ 边界情况测试
- ✅ 压力测试
- ✅ 迭代器测试

运行测试：
```bash
make run
> test libs
```

---

## 相关文档

- [RbTree API](./rbtree.md)
- [LRU Cache API](./lru.md)
- [RCU API](./rcu.md)
- [Radix Tree API](./rdtree.md)
- [B-Tree API](./btree.md)
- [Maple Tree API](./mptree.md)

---

**最后更新**: 2026-02-08
