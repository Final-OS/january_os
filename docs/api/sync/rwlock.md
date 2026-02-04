# RwLock - 读写锁

RwLock 允许多个读者或单个写者同时访问数据，适合读多写少的场景。

## API

```rust
pub struct RwLock<T: ?Sized> {
    // ...
}

impl<T> RwLock<T> {
    pub const fn new(value: T) -> Self
    pub fn read(&self) -> RwLockReadGuard<'_, T>
    pub fn write(&self) -> RwLockWriteGuard<'_, T>
}
```

## 使用示例

### 基本使用

```rust
use kernel::sync::RwLock;

static CONFIG: RwLock<Config> = RwLock::new(Config {
    debug: false,
    verbose: 0,
});

// 读取 (多个读者可并发)
fn print_config() {
    let config = CONFIG.read();
    kprintln!("Debug: {}", config.debug);
    kprintln!("Verbose: {}", config.verbose);
} // 自动释放读锁

// 写入 (独占访问)
fn set_debug(debug: bool) {
    let mut config = CONFIG.write();
    config.debug = debug;
} // 自动释放写锁
```

### 读多写少场景

```rust
use kernel::sync::RwLock;

struct Cache {
    data: Vec<u8>,
    valid: bool,
}

static CACHE: RwLock<Cache> = RwLock::new(Cache {
    data: Vec::new(),
    valid: false,
});

// 读取缓存 (频繁)
fn get_cached_data() -> Option<Vec<u8>> {
    let cache = CACHE.read();
    if cache.valid {
        Some(cache.data.clone())
    } else {
        None
    }
}

// 更新缓存 (不频繁)
fn update_cache(new_data: Vec<u8>) {
    let mut cache = CACHE.write();
    cache.data = new_data;
    cache.valid = true;
}
```

## 实现原理

### 读者/写者状态

```rust
struct RwLock<T> {
    state: AtomicU32,  // 包含读者计数和写者标志
    data: UnsafeCell<T>,
}

// 状态位:
// bit 0: 写者锁标志
// bit 1-31: 读者计数
const WRITER_LOCKED: u32 = 1;
const READER_MASK: u32 = !1;
const ONE_READER: u32 = 2;
```

### 获取读锁

```rust
pub fn read(&self) -> RwLockReadGuard<'_, T> {
    loop {
        let state = self.state.load(Ordering::Acquire);

        // 检查是否有写者
        if state & WRITER_LOCKED != 0 {
            core::hint::spin_loop();
            continue;
        }

        // 尝试增加读者计数
        match self.state.compare_exchange(
            state,
            state + ONE_READER,
            Ordering::Acquire,
            Ordering::Relaxed
        ) {
            Ok(_) => break,
            Err(_) => continue,
        }
    }

    RwLockReadGuard { lock: self }
}
```

### 获取写锁

```rust
pub fn write(&self) -> RwLockWriteGuard<'_, T> {
    // 设置写者标志
    while self.state.fetch_or(WRITER_LOCKED, Ordering::Acquire)
        & WRITER_LOCKED != 0
    {
        core::hint::spin_loop();
    }

    // 等待所有读者完成
    while self.state.load(Ordering::Acquire) & READER_MASK != 0 {
        core::hint::spin_loop();
    }

    RwLockWriteGuard { lock: self }
}
```

## 性能特点

| 操作 | 并发度 | 适用场景 |
|------|--------|----------|
| 读-读 | 高并发 | 多个读者同时访问 |
| 读-写 | 互斥 | 读者和写者互斥 |
| 写-写 | 互斥 | 写者之间互斥 |

## 使用场景

### 配置数据

```rust
use kernel::sync::RwLock;

struct SystemConfig {
    max_connections: u32,
    timeout_ms: u32,
    log_level: u8,
}

static CONFIG: RwLock<SystemConfig> = RwLock::new(SystemConfig {
    max_connections: 100,
    timeout_ms: 5000,
    log_level: 3,
});

// 读取配置 (频繁)
fn handle_connection() {
    let config = CONFIG.read();
    if config.max_connections > 0 {
        // 处理连接...
    }
}

// 修改配置 (不频繁)
fn set_max_connections(n: u32) {
    let mut config = CONFIG.write();
    config.max_connections = n;
}
```

### 统计数据

```rust
use kernel::sync::RwLock;

struct Stats {
    total_requests: u64,
    active_connections: u32,
    errors: u64,
}

static STATS: RwLock<Stats> = RwLock::new(Stats {
    total_requests: 0,
    active_connections: 0,
    errors: 0,
});

// 读取统计 (Web 界面等)
fn get_stats() -> (u64, u32, u64) {
    let stats = STATS.read();
    (stats.total_requests, stats.active_connections, stats.errors)
}

// 更新统计 (请求处理)
fn record_request() {
    let mut stats = STATS.write();
    stats.total_requests += 1;
}
```

### 缓存

```rust
use kernel::sync::RwLock;

struct LookupTable {
    entries: HashMap<u32, String>,
}

static TABLE: RwLock<LookupTable> = RwLock::new(LookupTable {
    entries: HashMap::new(),
});

// 查找 (频繁)
fn lookup(id: u32) -> Option<String> {
    let table = TABLE.read();
    table.entries.get(&id).cloned()
}

// 更新 (不频繁)
fn insert(id: u32, value: String) {
    let mut table = TABLE.write();
    table.entries.insert(id, value);
}
```

## 注意事项

### 1. 写饥饿

如果读者持续存在，写者可能饥饿。当前实现不防止写饥饿。

### 2. 读锁升级

```rust
// ❌ 错误: 不能直接升级读锁为写锁
let data = LOCK.read();
if need_write {
    drop(data);
    let mut data = LOCK.write();  // 先释放再获取
}
```

### 3. 递归

```rust
// ❌ 错误: 不支持递归
fn recursive_read() {
    let _guard1 = LOCK.read();
    let _guard2 = LOCK.read();  // 可能死锁
}
```

## 性能对比

| 场景 | RwLock | SpinLock |
|------|--------|----------|
| 只读 | 优秀 | 良好 |
| 读写混合 | 良好 | 良好 |
| 只写 | 良好 | 良好 |
| 内存开销 | 较大 | 较小 |

## 相关文档

- [SpinLock - 自旋锁](./spinlock.md)
- [Mutex - 互斥锁](./mutex.md)
- [Semaphore - 信号量](./semaphore.md)
