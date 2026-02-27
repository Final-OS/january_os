# fault - 页错误处理

页错误处理程序负责处理缺页异常，实现按需加载、写时复制和栈增长等功能。

## 概述

当访问无效或未映射的虚拟地址时，CPU 触发 #PF (Page Fault) 异常。

## 错误代码

```rust
pub struct PageErrorCode: u64 {
    const PRESENT = 1 << 0;   // 0 = 页不存在, 1 = 访问违规
    const WRITE   = 1 << 1;   // 0 = 读, 1 = 写
    const USER    = 1 << 2;   // 0 = 内核, 1 = 用户
    const RSVD    = 1 << 3;   // 保留位被设置
    const ID      = 1 << 4;   // 指令取
    const PK      = 1 << 5;   // 保护密钥
    const SGX     = 1 << 15;  // SGX 相关
}
```

## API

### 处理函数

```rust
pub fn handle_page_fault(ctx: &FaultContext) -> FaultResult
```

**上下文**：
```rust
pub struct FaultContext {
    pub addr: VirtAddr,       // 触发错误的地址
    pub error_code: PageErrorCode,
    pub ip: VirtAddr,         // 指令指针
}
```

**结果**：
```rust
pub enum FaultResult {
    Handled,      // 已处理
    NotPresent,   // 页不存在
    Permission,   // 权限不足
    Other,        // 其他错误
}
```

### 统计信息

```rust
pub fn get_fault_stats() -> FaultStats

pub struct FaultStats {
    pub total: u64,
    pub not_present: u64,
    pub permission: u64,
    pub cow: u64,
    pub stack_growth: u64,
}
```

**示例**：
```rust
use kernel::mm::fault::{get_fault_stats};

let stats = get_fault_stats();
kprintln!("Page faults: total={}, cow={}, stack={}",
    stats.total,
    stats.cow,
    stats.stack_growth);
```

## 处理流程

```
#PF 异常
    │
    ▼
page_fault_handler (中断处理程序)
    │
    ▼
读取 CR2 (故障地址)
    │
    ▼
handle_page_fault
    │
    ▼
检查地址有效性
    │
    ├─ 内核空间 ──► 检查 VMA
    │   │
    │   ├─ 找到 VMA ──► 处理
    │   │               │
    │   │               ├─ COW ──► 复制页
    │   │               ├─ 缺页 ──► 分配页
    │   │               └─ 权限 ──► 错误
    │   │
    │   └─ 未找到 ──► 内核 oops
    │
    └─ 用户空间 ──► 返回 FaultResult::Sigsegv / Sigbus
                     （后续由调度/信号层转为进程信号；当前中断层对未处理 fault 仍会 panic）
```

## 错误类型

### 缺页 (Not Present)

访问的页不存在，需要分配：

```rust
// 示例：处理缺页
if error_code.contains(PageErrorCode::PRESENT) == false {
    // 页不存在，需要分配
    if let Some(page) = alloc_pages(0, GFP_KERNEL) {
        // 映射到页表
        map_page(fault_addr, page, flags);
        return FaultResult::Handled;
    }
}
```

### 写时复制 (COW)

写入只读页，需要复制：

```rust
// 示例：处理 COW
if error_code.contains(PageErrorCode::WRITE) &&
   error_code.contains(PageErrorCode::PRESENT) {

    // 写入到只读页，检查是否 COW
    if vma.flags.contains(VmFlags::SHARED) == false {
        // 私有映射，复制页
        let new_page = alloc_pages(0, GFP_KERNEL)?;
        copy_page(old_page, new_page);

        // 重新映射为可写
        remap_page(fault_addr, new_page, READ | WRITE);
        return FaultResult::Handled;
    }
}
```

### 栈增长

访问栈下方，自动扩展：

```rust
// 示例：处理栈增长
if vma.flags.contains(VmFlags::GROWSDOWN) {
    let gap = vma.start.as_u64() - fault_addr.as_u64();

    if gap <= STACK_GAP_LIMIT {
        // 扩展栈 VMA
        grow_vma(vma, gap);
        return FaultResult::Handled;
    }
}
```

### 权限错误

违反访问权限：

```rust
// 示例：权限检查
if error_code.contains(PageErrorCode::WRITE) &&
   !vma.flags.contains(VmFlags::WRITE) {
    // 尝试写入只读区域
    return FaultResult::Permission;
}

if error_code.contains(PageErrorCode::ID) &&
   !vma.flags.contains(VmFlags::EXEC) {
    // 尝试执行不可执行区域
    return FaultResult::Permission;
}
```

## 中断处理程序

```rust
// kernel/src/interrupt/handlers.rs

extern "x86-interrupt" fn page_fault_handler(
    frame: InterruptFrame,
    error_code: u64
) {
    // 读取故障地址
    let fault_addr = VirtAddr::new(unsafe { read_cr2() });

    // 构建上下文
    let ctx = FaultContext {
        addr: fault_addr,
        error_code: PageErrorCode::from_bits_truncate(error_code),
        ip: frame.rip,
    };

    // 处理页错误
    match handle_page_fault(&ctx) {
        FaultResult::Handled => {
            // 已处理，返回继续执行
        }
        FaultResult::NotPresent => {
            // 页不存在
            page_error_not_present(ctx);
        }
        FaultResult::Permission => {
            // 权限不足
            page_error_permission(ctx);
        }
        FaultResult::Other => {
            // 其他错误
            page_error_other(ctx);
        }
    }
}
```

## 调试

### 启用追踪

```toml
# os_cfg.toml
[debug]
mm_debug = true
```

### 错误输出

```
!!! PAGE FAULT !!!
  Address:    0xdeadbeef
  Error:      0x02 (PRESENT=0, WRITE=0, USER=0)
  IP:         0xffffffff80001000
  Access:     Read
  Location:   Kernel

Page fault: not present
```

## 使用场景

### 按需加载

延迟分配内存，只在实际访问时分配：

```rust
// mmap 时不分配物理页
mm.mmap(&vma);  // 只创建 VMA

// 访问时触发页错误
let ptr = 0x40000000 as *mut u32;
unsafe { *ptr = 42 };  // 触发 #PF
// 页错误处理程序分配页
```

### 写时复制

fork 后共享只读页，写入时复制：

```rust
// fork 时共享页
child_page = parent_page;  // 只读映射

// 子进程写入时触发 COW
unsafe { *child_ptr = 100 };  // 触发 #PF
// 页错误处理程序复制页
```

### 栈自动增长

栈不足时自动扩展：

```rust
fn deep_recursion(count: u32) {
    if count == 0 {
        return;
    }

    let buffer = [0u8; 1024];  // 使用栈空间
    // 访问 buffer 可能触发栈增长
    deep_recursion(count - 1);
}
```

## 注意事项

1. **递归页错误**：页错误处理程序本身不能触发页错误
2. **原子上下文**：不能在原子上下文中睡眠
3. **内核 oops**：内核访问无效地址是严重错误
4. **性能影响**：频繁页错误会影响性能

## 相关文档

- [vma - 虚拟内存区域](./vma.md)
- [paging - 页表操作](./paging.md)
- [interrupt - 中断处理](../interrupt/interrupt.md)
