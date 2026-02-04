# Barrier - 屏障

Barrier 等待多个线程到达同步点后再继续。

## API

```rust
pub struct Barrier {
    n: usize,
    count: AtomicUsize,
    generation: AtomicUsize,
}

impl Barrier {
    pub const fn new(n: usize) -> Self
    pub fn wait(&self) -> BarrierWaitResult
}

pub struct ResettableBarrier {
    // 可重置的屏障
}

pub struct CountDownLatch {
    // 倒计时门栓
}
```

## 使用示例

### 基本使用

```rust
use kernel::sync::Barrier;

static BARRIER: Barrier = Barrier::new(3);

fn thread_func(id: usize) {
    kprintln!("Thread {} phase 1", id);
    do_work_phase1();

    // 等待所有线程到达
    BARRIER.wait();

    kprintln!("Thread {} phase 2", id);
    do_work_phase2();
}

// 主线程
spawn(|| thread_func(0));
spawn(|| thread_func(1));
spawn(|| thread_func(2));
```

### 返回值检查

```rust
use kernel::sync::Barrier;

static BARRIER: Barrier = Barrier::new(4);

fn worker_thread(is_leader: bool) {
    do_work();

    let result = BARRIER.wait();
    if result.is_leader() {
        // 只有一个线程返回 true
        cleanup_shared_resources();
    }

    next_phase();
}
```

### ResettableBarrier - 可重置

```rust
use kernel::sync::ResettableBarrier;

static BARRIER: ResettableBarrier = ResettableBarrier::new(4);

// 多次使用
fn multi_phase() {
    // Phase 1
    BARRIER.wait();
    phase1_work();

    // 重置
    BARRIER.reset();

    // Phase 2
    BARRIER.wait();
    phase2_work();
}
```

### CountDownLatch - 倒计时

```rust
use kernel::sync::CountDownLatch;

static LATCH: CountDownLatch = CountDownLatch::new(3);

fn worker() {
    do_work();
    LATCH.count_down();
}

fn main_thread() {
    // 启动 3 个 worker
    for _ in 0..3 {
        spawn(worker);
    }

    // 等待所有 worker 完成
    LATCH.wait();
    kprintln!("All workers done");
}
```

## 实现原理

### Barrier 实现

```rust
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct Barrier {
    n: usize,
    count: AtomicUsize,
    generation: AtomicUsize,
}

pub struct BarrierWaitResult {
    is_leader: bool,
}

impl Barrier {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            count: AtomicUsize::new(0),
            generation: AtomicUsize::new(0),
        }
    }

    pub fn wait(&self) -> BarrierWaitResult {
        let mut current_generation = self.generation.load(Ordering::Acquire);
        let mut count = self.count.fetch_add(1, Ordering::Relaxed) + 1;

        if count < self.n {
            // 不是最后一个，等待
            loop {
                let gen = self.generation.load(Ordering::Acquire);
                if gen != current_generation {
                    // 新的 generation，被唤醒
                    return BarrierWaitResult { is_leader: false };
                }
                core::hint::spin_loop();
            }
        } else {
            // 最后一个，唤醒其他
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            return BarrierWaitResult { is_leader: true };
        }
    }
}
```

### CountDownLatch 实现

```rust
pub struct CountDownLatch {
    count: AtomicUsize,
}

impl CountDownLatch {
    pub fn new(n: usize) -> Self {
        Self {
            count: AtomicUsize::new(n),
        }
    }

    pub fn count_down(&self) {
        if self.count.fetch_sub(1, Ordering::Release) == 1 {
            // 唤醒等待者
        }
    }

    pub fn wait(&self) {
        while self.count.load(Ordering::Acquire) > 0 {
            core::hint::spin_loop();
        }
    }
}
```

## 使用场景

### 并行计算

```rust
use kernel::sync::Barrier;

static BARRIER: Barrier = Barrier::new(4);

fn parallel_sort(data: &mut [u32]) {
    // Phase 1: 分区
    let mid = data.len() / 2;
    // 启动线程处理各分区...

    // 等待分区完成
    BARRIER.wait();

    // Phase 2: 合并
    merge_sorted(data);
}
```

### 批处理

```rust
use kernel::sync::CountDownLatch;

fn process_batch(items: &[Item]) {
    let latch = CountDownLatch::new(items.len());

    for item in items {
        let latch = &latch;
        spawn(move || {
            process_item(item);
            latch.count_down();
        });
    }

    // 等待所有处理完成
    latch.wait();
}
```

### 初始化同步

```rust
use kernel::sync::Barrier;

static INIT_BARRIER: Barrier = Barrier::new(num_cpus() + 1);

fn per_cpu_init() {
    init_my_cpu();
    INIT_BARRIER.wait();

    // 所有 CPU 初始化完成后继续
    start_processing();
}
```

### MapReduce

```rust
use kernel::sync::Barrier;

fn map_reduce(data: &[Data]) -> Result {
    let n = num_workers();
    let barrier = Barrier::new(n);
    let mut results = Vec::new();

    // Map 阶段
    for chunk in data.chunks(data.len() / n) {
        let barrier = &barrier;
        let results = &mut results;
        spawn(move || {
            let partial = process_chunk(chunk);
            results.push(partial);
            barrier.wait();
        });
    }

    // 等待所有 map 完成
    barrier.wait();

    // Reduce 阶段
    reduce_results(&results)
}
```

## BarrierWaitResult

```rust
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }

    pub fn is_last(&self) -> bool {
        self.is_leader
    }
}
```

## 注意事项

1. **线程数匹配**：等待的线程数必须等于 Barrier::new(n) 的 n
2. **自旋等待**：当前实现自旋等待，不适合长时间等待
3. **重用**：普通 Barrier 可无限次重用
4. **死锁**：线程数不足会导致死锁

## 性能考虑

| 操作 | 成本 |
|------|------|
| wait() | O(1) 原子操作 |
| 最后到达 | 唤醒其他线程 |
| 非最后到达 | 自旋等待 |

## 相关文档

- [Semaphore - 信号量](./semaphore.md)
- [Once - 一次性初始化](./once.md)
- [SpinLock - 自旋锁](./spinlock.md)
