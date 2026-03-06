# Task Management API

任务管理子系统提供进程和线程管理功能，并已按组件化宏内核规范重组为 `façade + 分层子目录骨架`。



## 核心结构

### Task

任务控制块（TCB），表示一个可调度的执行单元。

```rust
pub struct Task {
    pub id: TaskId,              // 任务 ID
    pub pid: ProcessId,          // 进程 ID
    pub ppid: ProcessId,         // 父进程 ID
    pub name: String,            // 任务名称
    pub context_sp: usize,       // 上下文栈指针
    pub kstack: KernelStack,     // 内核栈
    pub status: TaskStatus,      // 任务状态
    pub exit_code: Option<i32>,  // 退出码
}
```

### TaskStatus

任务状态枚举。

```rust
pub enum TaskStatus {
    Ready,    // 就绪
    Running,  // 运行中
    Blocked,  // 阻塞
    Exited,   // 已退出
}
```

### TaskId / ProcessId

任务和进程的唯一标识符。

```rust
pub struct TaskId(pub usize);
pub struct ProcessId(pub usize);
```

---

## 任务创建

### Task::new

创建新任务。

```rust
pub fn new(name: String, entry: usize) -> Option<Self>
```

**参数**:
- `name`: 任务名称
- `entry`: 入口函数地址

**返回**: `Option<Task>` - 成功返回 Some(task)，失败返回 None

**示例**:
```rust
let task = Task::new(
    String::from("my_task"),
    my_function as usize
).expect("Failed to create task");
```

### Task::new_kernel

创建内核线程（便捷方法）。

```rust
pub fn new_kernel(name: &str, entry: extern "C" fn()) -> Self
```

**参数**:
- `name`: 任务名称
- `entry`: 入口函数

**示例**:
```rust
let task = Task::new_kernel("worker", worker_thread);
```

### spawn_kernel_thread

全局函数，创建并启动内核线程。

```rust
pub fn spawn_kernel_thread(name: &str, entry: extern "C" fn()) -> Arc<Mutex<Task>>
```

**参数**:
- `name`: 线程名称
- `entry`: 入口函数

**返回**: `Arc<Mutex<Task>>` - 任务的共享引用

**示例**:
```rust
use crate::task::spawn_kernel_thread;

extern "C" fn my_thread() {
    loop {
        // 线程逻辑
    }
}

let task = spawn_kernel_thread("my_thread", my_thread);
```

---

## 任务查询

### current_task

获取当前正在运行的任务。

```rust
pub fn current_task() -> Option<Arc<Mutex<Task>>>
```

**返回**: `Option<Arc<Mutex<Task>>>` - 当前任务，如果不在任务上下文中则返回 None

**示例**:
```rust
if let Some(task) = current_task() {
    let t = task.lock();
    println!("Current task: {}", t.name);
}
```

### current_pid / current_ppid / current_tid

获取当前任务的 PID/PPID/TID。

```rust
pub fn current_pid() -> Option<ProcessId>
pub fn current_ppid() -> Option<ProcessId>
pub fn current_tid() -> Option<TaskId>
```

**示例**:
```rust
if let Some(pid) = current_pid() {
    println!("Current PID: {}", pid.0);
}
```

### find_task_by_pid

根据 PID 查找任务。

```rust
pub fn find_task_by_pid(pid: ProcessId) -> Option<Arc<Mutex<Task>>>
```

**参数**:
- `pid`: 进程 ID

**返回**: `Option<Arc<Mutex<Task>>>` - 找到的任务

---

## 任务控制

### exit_current_task

退出当前任务。

```rust
pub fn exit_current_task(exit_code: i32)
```

**参数**:
- `exit_code`: 退出码

**示例**:
```rust
exit_current_task(0); // 正常退出
```

### exit_current_process

退出当前进程（及其所有线程）。

```rust
pub fn exit_current_process(exit_code: i32)
```

**参数**:
- `exit_code`: 退出码

---

## 调度

### schedule

触发任务调度，切换到下一个就绪任务。

```rust
pub fn schedule()
```

**说明**:
- 从就绪队列取出下一个任务
- 保存当前任务上下文
- 切换到新任务
- 如果没有就绪任务，返回调用者

**示例**:
```rust
// 主动让出 CPU
schedule();
```

---

## 内核栈

### KernelStack

内核栈管理结构。

```rust
pub struct KernelStack {
    ptr: NonNull<u8>,
    layout: Layout,
}
```

**特性**:
- 大小: 32KB
- 对齐: 4096 字节
- 自动分配和释放
- Send + Sync 安全

### KernelStack::new

创建新的内核栈。

```rust
pub fn new() -> Option<Self>
```

### KernelStack::top

获取栈顶地址。

```rust
pub fn top(&self) -> usize
```

---

## 上下文切换

### do_switch

底层上下文切换函数（汇编实现）。

```rust
pub unsafe fn do_switch(prev_ctx: *mut usize, next_ctx: *const usize)
```

**参数**:
- `prev_ctx`: 保存当前上下文的位置
- `next_ctx`: 要切换到的上下文

**说明**:
- 保存所有通用寄存器
- 保存 rflags
- 切换栈指针
- 恢复新任务的寄存器

**注意**: 这是 unsafe 函数，通常不直接调用，而是通过 `schedule()` 调用。

---

## 使用示例

### 创建并运行内核线程

```rust
use crate::task::spawn_kernel_thread;

extern "C" fn worker_thread() {
    loop {
        // 执行工作
        println!("Worker thread running");

        // 主动让出 CPU
        crate::task::sched::schedule();
    }
}

// 创建线程
let task = spawn_kernel_thread("worker", worker_thread);
```

### 获取当前任务信息

```rust
use crate::task::{current_task, current_pid};

if let Some(task) = current_task() {
    let t = task.lock();
    println!("Task name: {}", t.name);
    println!("Task ID: {}", t.id.0);
    println!("Status: {:?}", t.status);
}

if let Some(pid) = current_pid() {
    println!("Process ID: {}", pid.0);
}
```

### 任务退出

```rust
use crate::task::exit_current_task;

extern "C" fn my_task() {
    // 执行任务
    println!("Task starting");

    // 完成工作
    println!("Task done");

    // 退出
    exit_current_task(0);
}
```

---

## 注意事项

1. **内核线程 vs 用户线程**
   - 当前只支持内核线程
   - 用户线程支持正在开发中

2. **上下文切换**
   - 所有锁必须在 `schedule()` 之前释放
   - 不要在持有锁时调用 `schedule()`

3. **任务生命周期**
   - 任务退出后会被标记为 Exited
   - 退出的任务不会被重新调度
   - 任务资源会在适当时机回收

4. **线程安全**
   - Task 结构体使用 `Arc<Mutex<Task>>` 共享
   - 访问任务数据需要先获取锁

---

## 相关 API

- [Scheduler API](scheduler.md) - 调度器接口
- [Processor API](processor.md) - Per-CPU 处理器状态
- [Context API](context.md) - 上下文管理

---

**最后更新**: 2026-02-08
