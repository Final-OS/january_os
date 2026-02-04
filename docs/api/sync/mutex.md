# Mutex - 互斥锁

Mutex 提供互斥访问，支持可能的睡眠等待（规划中）。

## API

```rust
pub struct Mutex<T: ?Sized> {
    // ...
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self
    pub fn lock(&self) -> MutexGuard<'_, T>
}

pub struct MutexGuard<'a, T: ?Sized> {
    // ...
}
```

## 使用示例

### 基本使用

```rust
use kernel::sync::Mutex;

// 创建 Mutex
static COUNTER: Mutex<u64> = Mutex::new(0);

// 访问数据
{
    let mut counter = COUNTER.lock();
    *counter += 1;
} // 自动释放
```

### 与 SpinLock 的区别

| 特性 | SpinLock | Mutex |
|------|----------|-------|
| 等待方式 | 自旋 | 睡眠 (规划) |
| 适用场景 | 短临界区 | 长临界区 |
| 中断上下文 | 可用 | 不可用 |
| 性能 | 短期快 | 长期好 |

## IrqMutex - 中断安全版本

```rust
pub struct IrqMutex<T: ?Sized> {
    // ...
}

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self
    pub fn lock(&self) -> IrqMutexGuard<'_, T>
}
```

**特点**：
- 获取锁时自动禁用中断
- 释放锁时恢复中断状态
- 适合中断和线程共享的数据

**示例**：
```rust
use kernel::sync::IrqMutex;

static SHARED_DATA: IrqMutex<u32> = IrqMutex::new(0);

// 在线程中
*SHARED_DATA.lock() += 1;

// 在中断处理程序中
extern "x86-interrupt" fn irq_handler(frame: InterruptFrame) {
    *SHARED_DATA.lock() += 1;  // 安全
    local_apic_eoi();
}
```

## 实现细节

### Mutex 实现

```rust
use kernel::sync::SpinLock;

pub struct Mutex<T> {
    // 内部使用 SpinLock
    // 未来版本可能支持睡眠
    inner: SpinLock<T>,
}

impl<T> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        // 目前等同于 SpinLock
        MutexGuard {
            guard: self.inner.lock(),
        }
    }
}
```

### IrqMutex 实现

```rust
use kernel::interrupt::without_interrupts;

pub struct IrqMutex<T> {
    inner: SpinLock<T>,
}

impl<T> IrqMutex<T> {
    pub fn lock(&self) -> IrqMutexGuard<'_, T> {
        // 禁用中断
        without_interrupts(|| {
            IrqMutexGuard {
                guard: self.inner.lock(),
            }
        })
    }
}
```

## 使用场景

### 可能睡眠的操作

```rust
use kernel::sync::Mutex;

struct Queue {
    items: Vec<u32>,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    items: Vec::new(),
});

fn process_item() {
    let mut queue = QUEUE.lock();

    // 复杂处理可能需要睡眠
    if !queue.items.is_empty() {
        let item = queue.items.remove(0);
        // 处理 item...
    }
}
```

### 中断安全共享

```rust
use kernel::sync::IrqMutex;

struct DeviceStats {
    interrupts: u64,
    errors: u64,
}

static STATS: IrqMutex<DeviceStats> = IrqMutex::new(DeviceStats {
    interrupts: 0,
    errors: 0,
});

// 中断处理
extern "x86-interrupt" fn irq_handler(frame: InterruptFrame) {
    let mut stats = STATS.lock();
    stats.interrupts += 1;

    // 处理中断...

    local_apic_eoi();
}

// 线程
fn print_stats() {
    let stats = STATS.lock();
    kprintln!("Interrupts: {}", stats.interrupts);
}
```

## 注意事项

1. **目前行为**：当前 Mutex 实现为自旋锁，不支持睡眠
2. **中断安全**：在中断中使用共享数据用 IrqMutex
3. **递归锁**：不支持递归获取同一把锁
4. **死锁**：避免多锁场景中的死锁

## 相关文档

- [SpinLock - 自旋锁](./spinlock.md)
- [RwLock - 读写锁](./rwlock.md)
- [Semaphore - 信号量](./semaphore.md)
