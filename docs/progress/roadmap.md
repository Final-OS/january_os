# january_os 完整路线图

本文定义从 v0.1 到 v1.0 正式发布的完整版本规划，目标是构建一个**功能完备、支持 x86-64/aarch64/riscv64 三种架构、全量兼容 Linux ABI** 的操作系统。

---

## 项目愿景

### 最终目标 (v1.0.0)

january_os v1.0 将是一个：

- **功能完备**：可运行真实 Linux 用户态程序（gcc, python, nginx, systemd 等）
- **多架构支持**：x86-64, aarch64, riscv64 三种架构完整支持
- **Linux ABI 兼容**：实现 300+ 核心 syscall，通过 Linux Test Project 基础测试
- **生产就绪**：具备完整的驱动、网络、文件系统、安全特性

### 版本总数与周期

| 阶段 | 版本范围 | 重点 | 状态 |
|------|----------|------|------|
| 基础建设 | v0.1 - v0.2 | 内核基础、进程/调度 | ✅ 已完成 |
| 存储与用户空间 | v0.3 - v0.4 | 文件系统、用户程序执行 | 🚧 进行中 |
| 网络与互联 | v0.5 - v0.6 | 网络栈、分布式基础 | 📋 计划中 |
| 多架构扩展 | v0.7 - v0.8 | aarch64/riscv64 移植 | 📋 计划中 |
| 安全与稳定 | v0.9 - v1.0 | 安全加固、生产就绪 | 📋 计划中 |

---

## 版本总览

```
v0.1.0 ✅ 内核基础           v0.6.0   网络栈完整
v0.2.0 ✅ 进程/调度/IPC      v0.7.0   aarch64 移植
v0.3.0 🚧 文件系统/块设备    v0.8.0   riscv64 移植
v0.4.0   用户空间完善        v0.9.0   安全加固
v0.5.0   网络栈基础          v1.0.0   正式发布
```

---

## v0.1.0 ✅ 已完成

**发布日期**: 2026-02

### 核心目标
构建可启动的内核基础框架。

### 功能清单

| 领域 | 功能 | 状态 | 关键文件 |
|------|------|------|----------|
| **引导** | UEFI 引导 | ✅ | `boot/x86_64/` |
| | GOP 图形输出 | ✅ | `boot/x86_64/src/gop.rs` |
| | BootInfo 传递 | ✅ | `boot/x86_64/src/info.rs` |
| | Higher-half 映射 | ✅ | `kernel/arch/x86_64/linker.ld` |
| **内存** | Memblock 早期分配 | ✅ | `kernel/src/mm/boot/memblock.rs` |
| | Buddy 分配器 | ✅ | `kernel/src/mm/alloc/buddy.rs` |
| | SLUB 分配器 | ✅ | `kernel/src/mm/alloc/slub.rs` |
| | PCP Per-CPU 缓存 | ✅ | `kernel/src/mm/alloc/pcp.rs` |
| | VMA 管理 | ✅ | `kernel/src/mm/virt/vma.rs` |
| | vmalloc/vfree | ✅ | `kernel/src/mm/virt/vmalloc.rs` |
| | Zone 管理 | ✅ | `kernel/src/mm/runtime/zone.rs` |
| | NUMA 支持 | ✅ | `kernel/src/mm/runtime/numa.rs` |
| | IOMMU (VT-d) | ✅ | `kernel/src/mm/dma/iommu.rs` |
| **中断** | GDT/TSS/IST | ✅ | `kernel/src/interrupt/arch/x86_64/gdt.rs` |
| | IDT 256 门 | ✅ | `kernel/src/interrupt/arch/x86_64/idt.rs` |
| | Local APIC | ✅ | `kernel/src/interrupt/arch/x86_64/apic.rs` |
| | I/O APIC | ✅ | `kernel/src/interrupt/arch/x86_64/ioapic.rs` |
| | APIC Timer | ✅ | `kernel/src/interrupt/arch/x86_64/timer.rs` |
| | 异常处理 | ✅ | `kernel/src/interrupt/handlers/` |
| **驱动** | PS/2 键盘 | ✅ | `kernel/src/drivers/input/keyboard.rs` |
| | 16550 UART | ✅ | `kernel/src/drivers/serial/uart.rs` |
| | Framebuffer 控制台 | ✅ | `kernel/src/drivers/tty/fbcon.rs` |
| | USB xHCI | ✅ | `kernel/src/drivers/usb/xhci/` |
| | USB HID | ✅ | `kernel/src/drivers/usb/hid/` |
| | ACPI 表解析 | ✅ | `kernel/src/drivers/acpi/` |
| **SMP** | AP 启动 | ✅ | `kernel/src/smp/arch/x86_64/` |
| | Per-CPU 数据 | ✅ | `kernel/src/smp/arch/x86_64/percpu.rs` |
| | IPI | ✅ | `kernel/src/smp/arch/x86_64/ipi.rs` |
| **数据结构** | 红黑树 | ✅ | `kernel/src/libs/rbtree.rs` |
| | B-Tree | ✅ | `kernel/src/libs/btree.rs` |
| | Maple Tree | ✅ | `kernel/src/libs/mptree.rs` |
| | Radix Tree | ✅ | `kernel/src/libs/rdtree.rs` |
| | LRU 缓存 | ✅ | `kernel/src/libs/lru.rs` |
| | RCU | ✅ | `kernel/src/libs/rcu.rs` |
| **同步** | SpinLock | ✅ | `kernel/src/sync/spinlock.rs` |
| | Mutex | ✅ | `kernel/src/sync/mutex.rs` |
| | RwLock | ✅ | `kernel/src/sync/rwlock.rs` |
| | Semaphore | ✅ | `kernel/src/sync/semaphore.rs` |
| | Once | ✅ | `kernel/src/sync/once.rs` |
| | Barrier | ✅ | `kernel/src/sync/barrier.rs` |

### 验收标准
- [x] UEFI 引导成功
- [x] 高半部内核映射
- [x] Buddy + SLUB 内存分配器
- [x] 中断处理（IDT/APIC）
- [x] 基础驱动（串口/键盘/帧缓冲）
- [x] SMP 多核启动
- [x] 内核数据结构（RbTree/LRU/RCU/RadixTree/BTree）

---

## v0.2.0 ✅ 已完成

**发布日期**: 2026-03

### 核心目标
实现进程管理和用户态切换能力。

### 功能清单

| 领域 | 功能 | 状态 | 关键文件 |
|------|------|------|----------|
| **进程** | Task 结构 (PCB/TCB) | ✅ | `kernel/src/task/` |
| | fork/clone/vfork | ✅ | `kernel/src/task/proc/fork.rs` |
| | COW 页复制 | ✅ | `kernel/src/task/proc/fork.rs` |
| | execve ELF 加载 | ✅ | `kernel/src/task/proc/exec.rs` |
| | ring3 切换 | ✅ | `kernel/src/task/arch/x86_64/` |
| | exit/exit_group | ✅ | `kernel/src/task/proc/exit.rs` |
| | wait4 | ✅ | `kernel/src/task/proc/wait.rs` |
| **调度** | Round-Robin | ✅ | `kernel/src/task/sched/` |
| | Per-CPU 队列 | ✅ | `kernel/src/task/sched/` |
| | work-stealing | ✅ | `kernel/src/task/sched/` |
| | 调度统计 | ✅ | `kernel/src/task/sched/stats.rs` |
| **信号** | kill/tkill/tgkill | ✅ | `kernel/src/task/proc/signal.rs` |
| | SIGCHLD/SIGTERM/SIGKILL | ✅ | `kernel/src/task/proc/signal.rs` |
| | SIGSTOP/SIGCONT | ✅ | `kernel/src/task/proc/signal.rs` |
| **IPC** | pipe/pipe2 | ✅ | `kernel/src/fs/backing/pipe.rs` |
| | 阻塞/非阻塞 | ✅ | `kernel/src/fs/backing/pipe.rs` |
| **Syscall** | syscall/sysret | ✅ | `kernel/src/syscall/arch/x86_64/` |
| | 40+ syscall 实现 | ✅ | `kernel/src/syscall/` |
| **同步** | CondVar | ✅ | `kernel/src/sync/condvar.rs` |

### 已实现 Syscall (v0.2)

| 编号 | 名称 | 状态 | 说明 |
|------|------|------|------|
| 0 | read | ✅ | 完整 |
| 1 | write | ✅ | 完整 |
| 2 | open | 🟡 | 仅 O_RDONLY |
| 3 | close | ✅ | 完整 |
| 7 | poll | ✅ | 完整 |
| 9 | mmap | 🟡 | 缺 MAP_SHARED |
| 10 | mprotect | 🟡 | 最小实现 |
| 11 | munmap | ✅ | 完整 |
| 12 | brk | ✅ | 完整 |
| 22 | pipe | ✅ | 完整 |
| 23 | select | ✅ | 完整 |
| 39 | getpid | ✅ | 完整 |
| 56 | clone | 🟡 | 最小子集 |
| 57 | fork | 🟡 | COW + 单线程 |
| 58 | vfork | ✅ | 完整 |
| 59 | execve | 🟡 | 最小实现 |
| 60 | exit | ✅ | 完整 |
| 61 | wait4 | ✅ | 完整 |
| 62 | kill | 🟡 | 部分信号 |
| ... | ... | ... | ... |

### 验收标准
- [x] 进程/线程创建（fork/clone/vfork）
- [x] ELF 加载与 ring3 切换（execve）
- [x] wait4 事件链路闭环
- [x] 信号机制（SIGCHLD/TERM/KILL/STOP/CONT）
- [x] 管道 IPC（pipe/pipe2）
- [x] Per-CPU 调度器 + 基础 work-stealing
- [x] 同步原语（Mutex/Semaphore/CondVar）
- [x] 40+ syscall 实现
- [x] 全量回归测试通过（44 项 [OK]）

---

## v0.3.0 🚧 进行中

**目标日期**: 2026-Q2

### 核心目标
**打通"磁盘 → 文件系统 → 用户程序"链路**

### 功能清单

| 领域 | 功能 | 状态 | 关键文件 |
|------|------|------|----------|
| **块设备** | BlockDevice trait | ✅ | `kernel/src/drivers/block/mod.rs` |
| | virtio-blk 驱动 | ✅ | `kernel/src/drivers/block/virtio_blk.rs` |
| | MBR 分区解析 | ✅ | `kernel/src/drivers/block/mbr.rs` |
| | GPT 分区解析 | ✅ | `kernel/src/drivers/block/gpt.rs` |
| | 分区设备包装 | ✅ | `kernel/src/drivers/block/partition.rs` |
| **VFS** | FileSystem trait | ✅ | `kernel/src/fs/api/` |
| | Inode trait | ✅ | `kernel/src/fs/api/inode.rs` |
| | File trait | ✅ | `kernel/src/fs/api/file.rs` |
| | mount/umount | ✅ | `kernel/src/fs/vfs/mount.rs` |
| | 路径解析 | ✅ | `kernel/src/fs/vfs/path.rs` |
| | fd 表 | ✅ | `kernel/src/fs/runtime/fd.rs` |
| **FAT32** | BPB/FSInfo 解析 | ✅ | `kernel/src/fs/backing/fat/` |
| | 目录遍历 | ✅ | `kernel/src/fs/backing/fat/dir.rs` |
| | 文件读取 | ✅ | `kernel/src/fs/backing/fat/file.rs` |
| | LFN 长文件名 | ✅ | `kernel/src/fs/backing/fat/` |
| | 写入 | 🔴 | v0.4 |
| **ext4** | 超级块解析 | ✅ | `kernel/src/fs/backing/ext4/` |
| | 组描述符 | ✅ | `kernel/src/fs/backing/ext4/` |
| | extent 读取 | ✅ | `kernel/src/fs/backing/ext4/` |
| | 目录遍历 | ✅ | `kernel/src/fs/backing/ext4/` |
| | htree 索引 | 🟡 | 基础 htree-root 兼容已接入，完整语义继续在 v0.3.2/v0.4 |
| | 写入 | 🔴 | v0.4 |
| **initramfs** | cpio 解析 | ✅ | `kernel/src/fs/backing/initramfs/` |
| | rootfs 挂载 | ✅ | `kernel/src/fs/runtime/manager.rs` |
| **用户空间** | VFS ELF 加载 | ✅ | `kernel/src/task/proc/exec.rs` |
| | argv/envp 栈帧 | 🟡 | 最小实现 |
| | auxv 栈帧 | 🟡 | 最小实现 |
| | 动态链接器 | 🔴 | v0.3.2 |
| **Syscall** | lseek(8) | ✅ | 最小实现 |
| | getdents64(217) | ✅ | 最小实现 |
| | dup(32) | ✅ | 最小实现 |
| | dup2(33) | ✅ | 最小实现 |
| | fcntl(72) | ✅ | 最小子集 |
| | chdir(80) | ✅ | 最小实现 |
| | getcwd(79) | ✅ | 最小实现 |
| | statfs(138) | 🔴 | v0.3.1 |
| | fstatfs(139) | 🔴 | v0.3.1 |
| | dup3(292) | 🔴 | v0.3.1 |
| | fchdir(81) | 🔴 | v0.3.1 |
| **procfs** | /proc/self | 🔴 | v0.3.1 |
| | /proc/cpuinfo | 🔴 | v0.3.1 |
| | /proc/meminfo | 🔴 | v0.3.1 |

### v0.3 子版本规划

#### v0.3.0 主链路（当前）

| 任务 | 状态 |
|------|------|
| virtio-blk + 分区 | ✅ |
| VFS 核心 (mount/path) | ✅ |
| FAT32 只读 | ✅ |
| ext4 只读 | ✅ |
| execve 走 VFS | ✅ |
| 主链路 syscall | ✅ |

#### v0.3.1（下阶段）

| 任务 | 优先级 |
|------|--------|
| procfs 基础 | 🔴 P0 |
| statfs/fstatfs | 🔴 P0 |
| dup3/fchdir | 🔴 P0 |
| virtio-scsi 最小链路 | 🟡 P1 |

#### v0.3.2

| 任务 | 优先级 |
|------|--------|
| 动态链接器最小集 | 🔴 P0 |
| sysfs 基础 | 🟡 P1 |
| ext4 htree 完整语义 | 🟡 P1 |
| PVSCSI 最小链路 | 🟡 P1 |
| NVMe 最小链路 | 🟡 P1 |
| AHCI 最小链路 | 🟡 P1 |

### 验收标准
- [x] virtio-blk 设备成功初始化
- [x] MBR/GPT 分区正确解析
- [x] FAT32 分区可挂载并读取
- [x] ext4 分区可挂载并读取
- [x] 可从 VFS 加载 ELF 文件
- [ ] 默认镜像包含数据盘（阻塞项）
- [ ] 端到端：启动 → 挂载 → 执行磁盘程序
- [x] `test block` 通过
- [x] `test vfs` 通过
- [x] `test fs fat32` 通过
- [x] `test fs ext4` 通过

### 关键产出
- `kernel/src/drivers/block/` - 块设备驱动
- `kernel/src/fs/vfs/` - VFS 核心
- `kernel/src/fs/backing/fat/` - FAT32 文件系统
- `kernel/src/fs/backing/ext4/` - ext4 文件系统
- `userland/` - 用户态程序

---

## v0.4.0 📋 计划中

**目标日期**: 2026-Q3

### 核心目标
**用户空间生态完善 + 可写文件系统**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **可写文件系统** | FAT32 写入 (create/write/delete) | 🔴 |
| | ext4 写入 (create/write/delete) | 🔴 |
| | 文件锁 (flock/fcntl F_SETLK) | 🟡 |
| | 符号链接 / 硬链接 | 🟡 |
| **Syscall 150+** | openat(257) | 🔴 |
| | rename(82) / unlink(87) | 🔴 |
| | mkdir(83) / rmdir(84) | 🔴 |
| | chmod(90) / fchmod(91) | 🟡 |
| | chown(92) / fchown(93) | 🟡 |
| | gettimeofday(96) | 🔴 |
| | clock_gettime(228) | 🔴 |
| | nanosleep(35) | 🔴 |
| | mprotect(10) 完善 | 🔴 |
| | madvise(28) | 🟡 |
| | msync(26) | 🔴 |
| | mlock(149) | 🟡 |
| | setuid(105) / setgid(106) | 🟡 |
| | setpgid(109) / setsid(112) | 🟡 |
| | rt_sigaction(13) 完善 | 🟡 |
| | rt_sigprocmask(14) 完善 | 🟡 |
| **ELF 加载** | 完整 PT_INTERP 支持 | 🔴 |
| | TLS (Thread Local Storage) | 🔴 |
| | PIE/PIC 支持 | 🟡 |
| | AT_* auxv 完整实现 | 🔴 |
| **initramfs** | cpio 格式解析完善 | 🟡 |
| | 启动时挂载根文件系统 | 🔴 |
| | /init 进程启动 | 🔴 |
| **用户程序** | init (PID 1) | 🔴 |
| | sh (基础 shell) | 🔴 |
| | ls / cat / echo / mkdir / rm | 🔴 |

### 验收标准
- [ ] 可创建/修改/删除文件
- [ ] 可运行 musl libc 编译的程序
- [ ] 可执行 shell 脚本
- [ ] `ls -la /` 正常工作
- [ ] syscall 覆盖率达到 150+

### 关键产出
- `kernel/src/fs/backing/fat/` - FAT32 写入
- `kernel/src/fs/backing/ext4/` - ext4 写入
- `userland/init/` - init 进程
- `userland/sh/` - shell

---

## v0.5.0 📋 计划中

**目标日期**: 2027-Q1

### 核心目标
**网络栈基础 + 可联网**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **网络驱动** | virtio-net 驱动 | 🔴 |
| | e1000/e1000e 驱动 | 🟡 |
| | NetworkDevice trait | 🔴 |
| **TCP/IP 协议栈** | 以太网层 (ARP) | 🔴 |
| | IPv4 (ICMP/UDP/TCP) | 🔴 |
| | IPv6 (基础支持) | 🟡 |
| | socket 缓冲区管理 | 🔴 |
| | 滑动窗口 | 🔴 |
| | 拥塞控制 (基础) | 🟡 |
| **Socket API** | socket(41) / bind(49) | 🔴 |
| | listen(50) / accept(43) | 🔴 |
| | connect(42) | 🔴 |
| | sendto(44) / recvfrom(45) | 🔴 |
| | sendmsg(46) / recvmsg(47) | 🟡 |
| | setsockopt(54) / getsockopt(55) | 🟡 |
| | getsockname(51) / getpeername(52) | 🟡 |
| | shutdown(48) | 🟡 |
| **网络配置** | DHCP 客户端 | 🟡 |
| | 静态 IP 配置 | 🔴 |
| | DNS 解析 (基础) | 🟡 |
| **网络工具** | ifconfig / ip (基础) | 🟡 |
| | ping | 🔴 |
| | nc (netcat 最小实现) | 🟡 |

### 验收标准
- [ ] QEMU 网络通信正常
- [ ] 可 ping 通宿主机
- [ ] 可建立 TCP 连接
- [ ] 简单 HTTP 客户端可用
- [ ] `test net` 通过

### 关键产出
- `kernel/src/net/` - 网络子系统
- `kernel/src/drivers/net/` - 网络驱动

---

## v0.6.0 📋 计划中

**目标日期**: 2027-Q2

### 核心目标
**网络栈完善 + 高级特性**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **TCP/IP 完善** | TCP 状态机完整实现 | 🔴 |
| | Nagle 算法 / Delayed ACK | 🟡 |
| | TCP Fast Open | 🟡 |
| | Zero-copy sendfile | 🟡 |
| **高级网络** | netlink socket (基础) | 🟡 |
| | unix domain socket | 🔴 |
| | packet socket (原始套接字) | 🟡 |
| | epoll(232) / epoll_ctl(233) | 🔴 |
| | epoll_wait(232) | 🔴 |
| | eventfd(284) | 🔴 |
| **网络文件系统** | NFS 客户端 (基础) | 🟡 |
| **更多 syscall** | signalfd(282) | 🟡 |
| | timerfd_create(283) | 🟡 |
| | inotify_init(253) | 🟡 |
| | inotify_add_watch(254) | 🟡 |
| | prctl(157) | 🟡 |
| | sysinfo(99) | 🔴 |
| | uname(63) | 🔴 |

### 验收标准
- [ ] HTTP 服务器可用（简单静态文件）
- [ ] SSH 客户端可用（dropbear）
- [ ] epoll 测试通过
- [ ] syscall 覆盖率达到 200+

---

## v0.7.0 📋 计划中

**目标日期**: 2027-Q3

### 核心目标
**aarch64 架构移植**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **aarch64 基础** | ARMv8-A 异常级别 (EL0-EL3) | 🔴 |
| | 页表格式 (4K/64K 页) | 🔴 |
| | GIC (Generic Interrupt Controller) | 🔴 |
| | ARM 定时器 | 🔴 |
| | 串口 (PL011) | 🔴 |
| **内存管理** | 页表映射 (TTBR0/TTBR1) | 🔴 |
| | TLB 管理 | 🔴 |
| | 缓存一致性 | 🔴 |
| | DMA 支持 | 🔴 |
| **进程/调度** | 上下文切换 | 🔴 |
| | 系统调用 (SVC 指令) | 🔴 |
| | 用户态切换 (ERET) | 🔴 |
| | 信号帧布局 | 🔴 |
| **驱动** | virtio-blk/net (MMIO) | 🔴 |
| | SD/MMC (可选) | 🟡 |
| | GPIO (基础) | 🟡 |
| **平台支持** | QEMU virt 机器 | 🔴 |
| | 树莓派 4 (基础启动) | 🟡 |

### 验收标准
- [ ] QEMU aarch64 启动成功
- [ ] v0.1-v0.6 功能在 aarch64 上全部可用
- [ ] 可执行 aarch64 用户程序
- [ ] `make run ARCH=aarch64` 正常工作

### 关键产出
- `kernel/src/arch/aarch64/` - aarch64 架构代码
- `boot/aarch64/` - aarch64 引导程序

---

## v0.8.0 📋 计划中

**目标日期**: 2027-Q4

### 核心目标
**riscv64 架构移植**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **riscv64 基础** | RISC-V 特权模式 (U/S/M) | 🔴 |
| | 页表格式 (Sv39/Sv48) | 🔴 |
| | PLIC (中断控制器) | 🔴 |
| | CLINT (核心本地中断器) | 🔴 |
| | RISC-V 定时器 | 🔴 |
| **内存管理** | 页表映射 (satp CSR) | 🔴 |
| | SFENCE.VMA | 🔴 |
| | 缓存刷新 | 🔴 |
| **进程/调度** | 上下文切换 | 🔴 |
| | 系统调用 (ecall) | 🔴 |
| | 用户态切换 (sret) | 🔴 |
| | 信号帧布局 | 🔴 |
| **驱动** | virtio-blk/net (MMIO) | 🔴 |
| | UART (ns16550a) | 🔴 |
| | HTIF (Host-Target Interface) | 🟡 |
| **平台支持** | QEMU virt 机器 | 🔴 |
| | SiFive Unmatched (可选) | 🟡 |

### 验收标准
- [ ] QEMU riscv64 启动成功
- [ ] v0.1-v0.6 功能在 riscv64 上全部可用
- [ ] 可执行 riscv64 用户程序
- [ ] `make run ARCH=riscv64` 正常工作

### 关键产出
- `kernel/src/arch/riscv64/` - riscv64 架构代码
- `boot/riscv64/` - riscv64 引导程序

---

## v0.9.0 📋 计划中

**目标日期**: 2028-Q1

### 核心目标
**安全加固 + 性能优化**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **安全特性** | ASLR (地址空间随机化) | 🔴 |
| | NX/DEP (不可执行保护) | 🔴 |
| | Stack Canary (栈保护) | 🔴 |
| | RELRO (重定位只读) | 🟡 |
| | 用户/内核隔离强化 | 🔴 |
| **权限系统** | POSIX capabilities | 🔴 |
| | setuid/setgid 程序支持 | 🔴 |
| | 基础审计日志 | 🟡 |
| **Syscall 250+** | 完整信号系统 | 🔴 |
| | 完整进程管理 | 🔴 |
| | 完整文件系统操作 | 🔴 |
| | 完整内存管理 | 🔴 |
| | 完整网络操作 | 🔴 |
| | ptrace(101) (基础调试) | 🟡 |
| **性能优化** | 大页支持 | 🟡 |
| | CFS/EEVDF 调度器 | 🟡 |
| | 文件系统缓存优化 | 🟡 |
| | 网络零拷贝 | 🟡 |
| **稳定性** | 压力测试 | 🔴 |
| | 长时间运行稳定性 | 🔴 |
| | 内存泄漏检测 | 🔴 |
| | 死锁检测 | 🔴 |

### 验收标准
- [ ] LTP 基础测试通过率 > 80%
- [ ] 可运行 gcc 编译简单程序
- [ ] 可运行 python 脚本
- [ ] 可运行 nginx 静态文件服务
- [ ] 三种架构全部通过回归测试

---

## v1.0.0 🎯 目标

**目标日期**: 2028-Q2

### 核心目标
**正式发布**

### 功能清单

| 领域 | 功能 | 优先级 |
|------|------|--------|
| **Linux ABI** | 300+ 核心 syscall | 🔴 |
| | LTP 测试通过率 > 95% | 🔴 |
| | 可运行主流 Linux 程序 | 🔴 |
| **三架构支持** | x86-64 生产就绪 | 🔴 |
| | aarch64 生产就绪 | 🔴 |
| | riscv64 生产就绪 | 🔴 |
| **完整驱动** | virtio 全家桶 | 🔴 |
| | AHCI/NVMe (x86-64) | 🔴 |
| | e1000/e1000e (x86-64) | 🔴 |
| | 基础 USB (xHCI) | 🔴 |
| **文档** | 内核 API 文档 | 🔴 |
| | 驱动开发指南 | 🟡 |
| | 用户手册 | 🔴 |
| | 架构设计文档 | 🟡 |
| **工具链** | 完整构建系统 | 🔴 |
| | 调试工具 (gdb stub) | 🟡 |
| | 性能分析工具 | 🟡 |
| | 系统监控工具 | 🟡 |

### 验收标准
- [ ] LTP 测试通过率 > 95%
- [ ] 可运行 systemd (基础)
- [ ] 可运行 gcc + make 构建项目
- [ ] 可运行 nginx + php-fpm
- [ ] 可运行 postgresql (基础)
- [ ] QEMU/VMware/真机启动
- [ ] 三种架构全部通过

### 发布物
- [ ] 源代码 (GitHub)
- [ ] 预编译镜像 (三种架构)
- [ ] SDK (交叉编译工具链)
- [ ] 文档网站

---

## Linux Syscall 覆盖路线图

| 版本 | 目标数量 | 覆盖范围 |
|------|----------|----------|
| v0.2 | 40 | 进程/内存/信号/pipe 基础 |
| v0.3 | 80 | 文件系统/VFS 基础 |
| v0.4 | 150 | 完整文件操作 + 时间 + 信号 |
| v0.5 | 180 | 网络 socket 基础 |
| v0.6 | 200 | 网络 + epoll + eventfd |
| v0.9 | 250 | 完整子集 + ptrace |
| v1.0 | 300+ | Linux ABI 兼容 |

### 核心 syscall 清单（v1.0 必须实现）

#### 进程管理
```
fork, vfork, clone, clone3, execve, execveat, exit, exit_group,
wait4, waitid, waitpid, getpid, getppid, gettid, setsid, getsid,
setpgid, getpgid, getpgrp, prctl, arch_prctl
```

#### 内存管理
```
mmap, munmap, mprotect, mremap, msync, madvise, mlock, munlock,
mlockall, munlockall, mincore, remap_file_pages, brk, sbrk
```

#### 文件系统
```
open, openat, openat2, close, read, write, readv, writev, pread,
pwrite, preadv, pwritev, lseek, llseek, dup, dup2, dup3, fcntl,
ioctl, flock, stat, fstat, lstat, fstatat, newfstatat, access,
faccessat, chmod, fchmod, fchmodat, chown, fchown, lchown, fchownat,
mkdir, mkdirat, rmdir, unlink, unlinkat, rename, renameat, renameat2,
link, linkat, symlink, symlinkat, readlink, readlinkat, creat,
truncate, ftruncate, getdents, getdents64, chdir, fchdir, getcwd,
sync, fsync, fdatasync, statfs, fstatfs, statx, utime, utimes,
futimesat, utimensat
```

#### 信号
```
signal, sigaction, sigprocmask, sigpending, sigsuspend, sigwaitinfo,
sigtimedwait, sigqueue, rt_sigaction, rt_sigprocmask, rt_sigpending,
rt_sigtimedwait, rt_sigqueueinfo, rt_tgsigqueueinfo, tgkill, kill,
tkill, pause
```

#### 网络
```
socket, bind, listen, accept, accept4, connect, getsockname,
getpeername, socketpair, send, sendto, sendmsg, sendmmsg, recv,
recvfrom, recvmsg, recvmmsg, shutdown, setsockopt, getsockopt
```

#### 时间
```
time, gettimeofday, settimeofday, clock_gettime, clock_settime,
clock_getres, nanosleep, clock_nanosleep, timer_create, timer_delete,
timer_settime, timer_gettime, timer_getoverrun
```

#### 同步
```
futex, futex_waitv, set_robust_list, get_robust_list
```

#### 其他
```
pipe, pipe2, socketpair, epoll_create, epoll_create1, epoll_ctl,
epoll_wait, epoll_pwait, select, pselect6, poll, ppoll, eventfd,
eventfd2, signalfd, signalfd4, timerfd_create, timerfd_settime,
timerfd_gettime, inotify_init, inotify_init1, inotify_add_watch,
inotify_rm_watch, uname, sysinfo, getrlimit, setrlimit, prlimit,
getrusage, getpid, getuid, geteuid, getgid, getegid, setuid, setgid,
setreuid, setregid, setresuid, setresgid, getresuid, getresgid,
capget, capset
```

---

## 多架构支持矩阵

| 特性 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 启动 | v0.1 ✅ | v0.7 | v0.8 |
| 内存管理 | v0.1 ✅ | v0.7 | v0.8 |
| 进程/调度 | v0.2 ✅ | v0.7 | v0.8 |
| 文件系统 | v0.3 🚧 | v0.7 | v0.8 |
| 网络 | v0.5 | v0.7 | v0.8 |
| 生产就绪 | v1.0 | v1.0 | v1.0 |

---

## 风险与缓解

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| 多架构工作量超预期 | 延期 v0.7/v0.8 | 中 | 优先保证 x86-64 稳定，架构代码充分抽象 |
| syscall 数量庞大 | 延期 v1.0 | 高 | 分版本逐步实现，优先高频 syscall |
| 性能不达标 | 影响可用性 | 中 | v0.9 专项性能优化，参考 Linux 调优 |
| 兼容性问题 | 用户程序运行失败 | 高 | 持续 LTP 测试，参考 glibc/strace |
| FAT32/ext4 复杂度 | v0.3 延期 | 中 | 先只读，限制特性集 |
| 动态链接器复杂 | v0.3.2 延期 | 中 | 最小子集，延后完整支持 |

---

## 里程碑总结

```
2026-Q1  v0.1 ✅  内核基础
2026-Q2  v0.2 ✅  进程/调度/IPC
2026-Q3  v0.3 🚧  文件系统/块设备
2026-Q4  v0.4    用户空间完善
2027-Q1  v0.5    网络栈基础
2027-Q2  v0.6    网络栈完善
2027-Q3  v0.7    aarch64 移植
2027-Q4  v0.8    riscv64 移植
2028-Q1  v0.9    安全加固
2028-Q2  v1.0    正式发布
```

---

## 当前进度

**版本**: v0.2.0 ✅ → v0.3.0 🚧

**v0.3.0 状态**:
- ✅ Batch 1: 块设备抽象 + virtio-blk
- ✅ Batch 2: 分区表解析 (MBR/GPT)
- ✅ Batch 3: VFS 核心抽象
- ✅ Batch 4: FAT32 只读
- ✅ Batch 4B: ext4 只读
- ✅ Batch 5: VFS syscall 接入
- ✅ Batch 6: ELF 加载完善
- 🔴 Batch 7: procfs (下放 v0.3.1)
- 🔴 Batch 8: 集成测试 (阻塞: 默认镜像无数据盘)
- 🔴 Batch 9: 文档收口

**下一步行动**:
1. 补齐默认数据盘镜像
2. v0.3.1: procfs + 扩展 syscall + virtio-scsi
3. v0.3.2: 动态链接器 + sysfs + ext4 增强

---

## 相关文档

- [开发进度](./overview.md) - 当前状态与已完成功能
- [技术债务](./tech-debt.md) - 债务清单与偿还计划
- [v0.3 计划](./v0.3-plan.md) - 当前版本分批实施计划
