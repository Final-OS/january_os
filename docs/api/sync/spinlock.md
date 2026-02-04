# SpinLock - 自旋锁

SpinLock 是最简单的同步原语，通过忙等待实现互斥访问。

## API

```rust
pub struct SpinLock<T: ?Sized> {
    // ...
}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self
    pub fn lock(&self) -> SpinLockGuard<'_, T>
}

impl<'a, T: ?Sized> SpinLockGuard<'a, T> {
    // 自动 Deref 到 T
}
```

## 使用示例

### 基本使用

```rust
use kernel::sync::SpinLock;

// 创建锁
static COUNTER: SpinLock<u64> = SpinLock::new(0);

// 访问数据
{
    let mut counter = COUNTER.lock();
    *counter += 1;
} // 自动释放

// 或使用
*COUNTER.lock() += 1;
```

### 复杂类型

```rust
use kernel::sync::SpinLock;

struct MyStruct {
    value: u32,
    name: [u8; 32],
}

static DATA: SpinLock<MyStruct> = SpinLock::new(MyStruct {
    value: 0,
    name: [0; 32],
});

// 修改
{
    let mut data = DATA.lock();
    data.value = 42;
    data.name[0] = b'A';
}
```

### 返回引用

```rust
use kernel::sync::SpinLock;

static BUFFER: SpinLock<[u8; 256]> = SpinLock::new([0; 256]);

fn get_buffer_ptr() -> *const u8 {
    BUFFER.lock().as_ptr()
}
```

## 实现细节

### 原子操作

```rust
use core::sync::atomic::{AtomicBool, Ordering};

struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        // 自旋获取锁
        while self.locked.compare_exchange(
            false, true,
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_err() {
            core::hint::spin_loop();
        }

        SpinLockGuard { lock: self }
    }
}
```

### Guard

```rust
pub struct SpinLockGuard<'a, T: ?Sized> {
    lock: &'a SpinLock<T>,
}

impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        // 释放锁
        self.lock.locked.store(false, Ordering::Release);
    }
}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}
```

## 使用场景

### 短临界区

```rust
use kernel::sync::SpinLock;

static LIST: SpinLock<Vec<&str>> = SpinLock::new(Vec::new());

fn add_item(item: &'static str) {
    let mut list = LIST.lock();
    list.push(item);
} // 快速释放
```

### 中断上下文

```rust
use kernel::sync::SpinLock;
use kernel::interrupt::without_interrupts;

static SHARED_DATA: SpinLock<u32> = SpinLock::new(0);

extern "x86-interrupt" fn irq_handler(frame: InterruptFrame) {
    // 在中断中使用 SpinLock
    *SHARED_DATA.lock() += 1;

    // 注意: 可能需要禁用中断
    // without_interrupts(|| {
    //     *SHARED_DATA.lock() += 1;
    // });

    local_apic_eoi();
}
```

### 简单状态共享

```rust
use kernel::sync::SpinLock;

struct DeviceState {
    initialized: bool,
    error_count: u32,
}

static STATE: SpinLock<DeviceState> = SpinLock::new(DeviceState {
    initialized: false,
    error_count: 0,
});

fn init_device() {
    let mut state = STATE.lock();
    if state.initialized {
        return;
    }

    // 初始化设备...
    state.initialized = true;
}
```

## 注意事项

### 1. 不能睡眠

```rust
// ❌ 错误: 在持有 SpinLock 时睡眠
let guard = SPIN_LOCK.lock();
some_function_that_might_sleep(); // 可能在锁内睡眠
drop(guard);
```

### 2. 死锁风险

```rust
// ❌ 错误: 可能死锁
fn deadlock_example() {
    let guard1 = LOCK1.lock();
    let guard2 = LOCK2.lock(); // 如果其他线程反向获取则死锁
}

// ✅ 正确: 始终按相同顺序获取锁
fn correct_example() {
    let guard1 = LOCK1.lock();
    let guard2 = LOCK2.lock();
}
```

### 3. 中断安全

```rust
use kernel::interrupt::without_interrupts;

// 中断处理程序可能抢占当前代码
// 如果中断处理程序也需要同一把锁则死锁

// 方案 1: 在中断中禁用中断
without_interrupts(|| {
    *DATA.lock() += 1;
});

// 方案 2: 使用 IrqMutex (禁用中断的自旋锁)
```

## 性能考虑

| 场景 | SpinLock | Mutex |
|------|----------|-------|
| 短临界区 | 好 | 差 |
| 长临界区 | 差 (浪费 CPU) | 好 |
| 中断上下文 | 必需 | 不可用 |
| 可能睡眠 | 不可用 | 可用 |

## 相关文档

- [Mutex - 互斥锁](./mutex.md)
- [RwLock - 读写锁](./rwlock.md)
- [Once - 一次性初始化](./once.md)
