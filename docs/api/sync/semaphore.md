# Semaphore - 信号量

Semaphore 是一个计数信号量，用于控制对有限资源的访问。

## API

```rust
pub struct Semaphore {
    permits: AtomicI32,
}

impl Semaphore {
    pub const fn new(permits: i32) -> Self
    pub fn acquire(&self) -> SemaphorePermit<'_>
    pub fn try_acquire(&self) -> Option<SemaphorePermit<'_>>
    pub fn release(&self)
}

pub struct BoundedSemaphore {
    // 有界信号量
}

pub struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
}
```

## 使用示例

### 基本使用

```rust
use kernel::sync::Semaphore;

// 创建信号量 (最多 3 个并发访问)
static SEM: Semaphore = Semaphore::new(3);

fn access_resource() {
    // 获取许可
    let _permit = SEM.acquire();

    // 访问资源...
    do_work();

} // 自动释放许可
```

### 尝试获取

```rust
use kernel::sync::Semaphore;

static SEM: Semaphore = Semaphore::new(2);

fn try_access() -> bool {
    // 非阻塞尝试获取
    if let Some(_permit) = SEM.try_acquire() {
        // 成功获取
        do_work();
        true
    } else {
        // 无可用许可
        false
    }
}
```

### 资源池管理

```rust
use kernel::sync::Semaphore;

struct ConnectionPool {
    connections: Vec<Connection>,
    semaphore: Semaphore,
}

impl ConnectionPool {
    fn new(max_connections: usize) -> Self {
        Self {
            connections: Vec::new(),
            semaphore: Semaphore::new(max_connections as i32),
        }
    }

    fn get_connection(&self) -> Option<Connection> {
        // 获取许可
        let _permit = self.semaphore.try_acquire()?;

        // 返回连接
        // 实际实现需要更复杂的逻辑
        Some(Connection::new())
    }
}
```

### 并发限制

```rust
use kernel::sync::Semaphore;

// 限制同时运行的请求数
static MAX_REQUESTS: Semaphore = Semaphore::new(10);

fn handle_request() {
    // 获取许可
    let _permit = MAX_REQUESTS.acquire();

    // 处理请求...
    process_request();

} // 自动释放
```

## BoundedSemaphore

有界信号量，防止许可泄漏。

```rust
use kernel::sync::BoundedSemaphore;

static SEM: BoundedSemaphore = BoundedSemaphore::new(5);

fn use_resource() {
    let _permit = SEM.acquire();
    // 使用资源...
}
```

## 实现原理

```rust
use core::sync::atomic::{AtomicI32, Ordering};

pub struct Semaphore {
    permits: AtomicI32,
}

impl Semaphore {
    pub const fn new(permits: i32) -> Self {
        Self {
            permits: AtomicI32::new(permits),
        }
    }

    pub fn acquire(&self) -> SemaphorePermit<'_> {
        // 自旋等待可用许可
        while self.permits.fetch_sub(1, Ordering::Acquire) <= 0 {
            // 失败，恢复计数
            self.permits.fetch_add(1, Ordering::Relaxed);
            core::hint::spin_loop();
        }

        SemaphorePermit {
            semaphore: self,
        }
    }

    pub fn try_acquire(&self) -> Option<SemaphorePermit<'_>> {
        // 尝试获取许可
        if self.permits.fetch_sub(1, Ordering::Acquire) > 0 {
            Some(SemaphorePermit {
                semaphore: self,
            })
        } else {
            // 恢复计数
            self.permits.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        // 释放许可
        self.semaphore.permits.fetch_add(1, Ordering::Release);
    }
}
```

## 使用场景

### 连接池

```rust
use kernel::sync::Semaphore;

struct DbConnectionPool {
    max_connections: usize,
    semaphore: Semaphore,
}

impl DbConnectionPool {
    fn new(max: usize) -> Self {
        Self {
            max_connections: max,
            semaphore: Semaphore::new(max as i32),
        }
    }

    fn get(&self) -> Option<Connection> {
        let _permit = self.semaphore.try_acquire()?;
        Some(Connection::new())
    }
}
```

### 并发任务限制

```rust
use kernel::sync::Semaphore;

static MAX_CONCURRENT: Semaphore = Semaphore::new(4);

fn spawn_tasks() {
    for i in 0..10 {
        // 每次最多 4 个任务并发
        let _permit = MAX_CONCURRENT.acquire();

        spawn_task(|| {
            do_work(i);
        });
    }
}
```

### 生产者-消费者

```rust
use kernel::sync::Semaphore;

struct BoundedQueue<T> {
    items: Vec<Option<T>>,
    head: usize,
    tail: usize,
    empty: Semaphore,
    full: Semaphore,
}

impl<T> BoundedQueue<T> {
    fn new(capacity: usize) -> Self {
        Self {
            items: (0..capacity).map(|_| None).collect(),
            head: 0,
            tail: 0,
            empty: Semaphore::new(capacity as i32),
            full: Semaphore::new(0),
        }
    }

    fn enqueue(&mut self, item: T) {
        let _permit = self.empty.acquire();
        // 添加 item...
    }

    fn dequeue(&mut self) -> T {
        let _permit = self.full.acquire();
        // 取出 item...
    }
}
```

## Semaphore vs Mutex

| 特性 | Semaphore | Mutex |
|------|-----------|-------|
| 计数 | 可大于 1 | 总是 1 |
| 用途 | 资源计数 | 互斥访问 |
| 获取/释放 | 可分离 | 必须配对 |

## 相关文档

- [Mutex - 互斥锁](./mutex.md)
- [SpinLock - 自旋锁](./spinlock.md)
- [Barrier - 屏障](./barrier.md)
