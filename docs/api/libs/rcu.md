# RCU API

`RCU`（Read-Copy-Update）用于读多写少场景的低开销并发访问。

## 关键能力

- 无锁读取
- 更新后延迟回收
- 宽限期同步

## 示例

```rust
use crate::libs::rcu::Rcu;

let rcu = Rcu::new(1u64);
let _guard = rcu.read();
let _old = rcu.update(2);
```

## 返回

- [数据结构概览](./overview.md)
