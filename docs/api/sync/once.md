# Once - 一次性初始化

Once 确保代码只执行一次，用于全局初始化。

## API

```rust
pub struct Once {
    state: AtomicU8,
}

impl Once {
    pub const fn new() -> Self
    pub fn call_once<F>(&self, f: F)
    where F: FnOnce()
}

pub struct OnceCell<T> {
    // ...
}

impl<T> OnceCell<T> {
    pub const fn new() -> Self
    pub fn set(&self, value: T) -> Result<(), T>
    pub fn get(&self) -> Option<&T>
}
```

## 使用示例

### Once 基本使用

```rust
use kernel::sync::Once;

static INIT: Once = Once::new();

fn expensive_init() {
    INIT.call_once(|| {
        // 只执行一次
        kprintln!("Initializing...");
        setup_system();
    });
}
```

### 多线程安全

```rust
use kernel::sync::Once;

static INIT: Once = Once::new();

fn thread_func() {
    INIT.call_once(|| {
        // 无论多少个线程调用，只执行一次
        kprintln!("Thread init");
    });

    // 其他代码...
}

// 多个线程调用
spawn(|| thread_func());
spawn(|| thread_func());
spawn(|| thread_func());
// "Thread init" 只打印一次
```

### OnceCell - 懒初始化

```rust
use kernel::sync::OnceCell;

static EXPENSIVE_DATA: OnceCell<Vec<u8>> = OnceCell::new();

fn get_data() -> &'static Vec<u8> {
    EXPENSIVE_DATA.get_or_init(|| {
        // 首次访问时初始化
        kprintln!("Computing expensive data...");
        vec![0; 1024 * 1024]
    })
}

// 或手动设置
fn init_data() {
    EXPENSIVE_DATA.set(vec![1, 2, 3]).unwrap();
}
```

### 复杂初始化

```rust
use kernel::sync::OnceCell;

struct GlobalState {
    config: Config,
    allocator: Allocator,
    cache: Cache,
}

static STATE: OnceCell<GlobalState> = OnceCell::new();

fn init_state() {
    STATE.get_or_init(|| {
        GlobalState {
            config: load_config(),
            allocator: Allocator::new(),
            cache: Cache::new(),
        }
    });
}

fn get_config() -> &'static Config {
    &STATE.get().unwrap().config
}
```

## 实现原理

### Once 状态

```rust
use core::sync::atomic::{AtomicU8, Ordering};

pub struct Once {
    state: AtomicU8,
}

// 状态值
const DONE: u8 = 2;
const IN_PROGRESS: u8 = 1;
const UNINITIALIZED: u8 = 0;

impl Once {
    pub fn call_once<F>(&self, f: F)
    where F: FnOnce()
    {
        // 快速路径：已完成
        if self.state.load(Ordering::Acquire) == DONE {
            return;
        }

        // 慢速路径：需要初始化
        self.call_once_slow(f);
    }
}
```

### OnceCell 实现

```rust
pub struct OnceCell<T> {
    once: Once,
    data: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Sync for OnceCell<T> {}

impl<T> OnceCell<T> {
    pub fn get_or_init<F>(&self, f: F) -> &T
    where F: FnOnce() -> T
    {
        // 快速路径
        if let Some(data) = self.get() {
            return data;
        }

        // 慢速路径
        self.once.call_once(|| {
            let value = f();
            unsafe {
                self.data.get().write(Some(value));
            }
        });

        unsafe {
            self.get().unwrap_unchecked()
        }
    }
}
```

## 使用场景

### 全局配置

```rust
use kernel::sync::OnceCell;

struct Config {
    debug: bool,
    verbose: u8,
}

static CONFIG: OnceCell<Config> = OnceCell::new();

fn init_from_file() {
    let config = Config {
        debug: read_bool("debug"),
        verbose: read_u8("verbose"),
    };
    CONFIG.set(config).ok();
}
```

### 缓存

```rust
use kernel::sync::OnceCell;

static LOOKUP_TABLE: OnceCell<HashMap<u32, String>> = OnceCell::new();

fn lookup(id: u32) -> Option<&'static str> {
    LOOKUP_TABLE.get_or_init(|| {
        build_lookup_table()
    }).get(&id).map(|s| s.as_str())
}
```

### 设备初始化

```rust
use kernel::sync::Once;

static DEVICE_INIT: Once = Once::new();

fn init_device() {
    DEVICE_INIT.call_once(|| {
        // 复杂的设备初始化
        reset_device();
        load_firmware();
        setup_interrupts();
        enable_device();
    });
}
```

### 算法表

```rust
use kernel::sync::OnceCell;

static CRC_TABLE: OnceCell<[u16; 256]> = OnceCell::new();

fn compute_crc(data: &[u8]) -> u16 {
    let table = CRC_TABLE.get_or_init(|| {
        // 只计算一次
        generate_crc_table()
    });

    // 使用表计算 CRC
    let mut crc = 0xFFFF;
    for &byte in data {
        crc = table[(crc ^ byte as u16) as usize];
    }
    crc
}
```

## 注意事项

### 1. 初始化失败

```rust
use kernel::sync::Once;

static INIT: Once = Once::new();

fn init_with_error() {
    INIT.call_once(|| {
        // 如果这里 panic，后续调用会检测到
        // 但可能无法恢复
        might_panic();
    });
}
```

### 2. 递归

```rust
use kernel::sync::Once;

static INIT: Once = Once::new();

fn recursive_init() {
    INIT.call_once(|| {
        // ❌ 死锁: 在 call_once 中再次调用
        recursive_init();
    });
}
```

### 3. 闭包捕获

```rust
use kernel::sync::OnceCell;

// ❌ 错误: 闭包生命周期不够长
fn init_temp<'a>() {
    let data = String::from("temp");
    CELL.get_or_init(|| data);  // 编译错误
}

// ✅ 正确: 使用 'static 数据
fn init_static() {
    let data = Box::leak(Box::new(String::from("static")));
    CELL.get_or_init(|| *data);
}
```

## 性能

| 操作 | 成本 |
|------|------|
| 已初始化后读取 | 单次 load |
| 首次初始化 | 原子操作 + 闭包 |
| 并发竞争 | 自旋等待 |

## 相关文档

- [SpinLock - 自旋锁](./spinlock.md)
- [Mutex - 互斥锁](./mutex.md)
- [Barrier - 屏障](./barrier.md)
