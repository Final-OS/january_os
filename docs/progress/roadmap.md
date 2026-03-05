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

| 阶段 | 版本范围 | 重点 |
|------|----------|------|
| 基础建设 | v0.1 - v0.2 | ✅ 已完成 |
| 存储与用户空间 | v0.3 - v0.4 | 文件系统、用户程序执行 |
| 网络与互联 | v0.5 - v0.6 | 网络栈、分布式基础 |
| 多架构扩展 | v0.7 - v0.8 | aarch64/riscv64 移植 |
| 安全与稳定 | v0.9 - v1.0 | 安全加固、生产就绪 |

---

## 版本总览

```
v0.1.0 ✅ 内核基础           v0.6.0   网络栈完整
v0.2.0 ✅ 进程/调度/IPC      v0.7.0   aarch64 移植
v0.3.0   文件系统/块设备     v0.8.0   riscv64 移植
v0.4.0   用户空间完善        v0.9.0   安全加固
v0.5.0   网络栈基础          v1.0.0   正式发布
```

---

## v0.1.0 ✅ 已完成

**发布日期**: 2026-02

### 核心目标
构建可启动的内核基础框架。

### 验收标准
- [x] UEFI 引导成功
- [x] 高半部内核映射
- [x] Buddy + SLUB 内存分配器
- [x] 中断处理（IDT/APIC）
- [x] 基础驱动（串口/键盘/帧缓冲）
- [x] SMP 多核启动
- [x] 内核数据结构（RbTree/LRU/RCU/RadixTree/BTree）

### 关键产出
- `boot/x86_64/` - UEFI 引导程序
- `kernel/src/mm/` - 内存管理子系统
- `kernel/src/drivers/` - 基础驱动
- `kernel/src/libs/` - 内核数据结构

---

## v0.2.0 ✅ 已完成

**发布日期**: 2026-03

### 核心目标
实现进程管理和用户态切换能力。

### 验收标准
- [x] 进程/线程创建（fork/clone/vfork）
- [x] ELF 加载与 ring3 切换（execve）
- [x] wait4 事件链路闭环
- [x] 信号机制（SIGCHLD/TERM/KILL/STOP/CONT）
- [x] 管道 IPC（pipe/pipe2）
- [x] Per-CPU 调度器 + 基础 work-stealing
- [x] 同步原语（Mutex/Semaphore/CondVar）
- [x] 40+ syscall 实现

### 关键产出
- `kernel/src/task/` - 进程/线程管理
- `kernel/src/syscall/` - 系统调用框架
- `kernel/src/sync/` - 同步原语

---

## v0.3.0 📋 计划中

**目标日期**: 2026-Q2

### 核心目标
**打通"磁盘 → 文件系统 → 用户程序"链路**

### 必须完成（不可延期）

#### 1. 块设备层
- [ ] `BlockDevice` trait 抽象
- [ ] virtio-blk 驱动（完整读写）
- [ ] MBR/GPT 分区表解析
- [ ] 通用块层（请求队列、合并、调度）

#### 2. VFS 核心
- [ ] `FileSystem` / `Inode` / `File` / `Dentry` trait
- [ ] mount/umount 管理
- [ ] 路径解析（绝对/相对、`.` `..`）
- [ ] 文件描述符表重构（`Arc<dyn File>`）

#### 3. FAT32 文件系统
- [ ] BPB/FSInfo 解析
- [ ] 目录遍历（readdir）
- [ ] 文件读取（read）
- [ ] **只读**挂载

#### 4. ext4 文件系统（只读）
- [ ] 超级块/组描述符解析
- [ ] extent tree 遍历
- [ ] 目录索引（htree）
- [ ] 文件读取

#### 5. 用户空间完善
- [ ] argv/envp/auxv 栈帧构建
- [ ] 从 VFS 加载 ELF 文件
- [ ] 动态链接器（ld.so 最小实现）
- [ ] musl libc 移植（最小子集）

#### 6. 新增 syscall
- [ ] lseek(8)
- [ ] getdents64(217)
- [ ] statfs(138) / fstatfs(139)
- [ ] dup(32) / dup2(33) / dup3(292)
- [ ] fcntl(72)
- [ ] access(21) / faccessat(269)
- [ ] chdir(80) / fchdir(81) / getcwd(79)

#### 7. procfs
- [ ] /proc/self
- [ ] /proc/[pid]/{cmdline,status,maps,fd}
- [ ] /proc/cpuinfo / meminfo / version

### 验收标准
- [ ] 可挂载 FAT32/ext4 分区
- [ ] 可执行磁盘上的静态链接 ELF 程序
- [ ] 可执行动态链接程序（hello world）
- [ ] `test block` / `test vfs` / `test fat` / `test ext4` 全部通过
- [ ] 在 shell 中运行 `ls /` `cat /proc/cpuinfo`

### 关键产出
- `kernel/src/drivers/block/` - 块设备驱动
- `kernel/src/fs/vfs/` - VFS 核心
- `kernel/src/fs/fat/` - FAT32 文件系统
- `kernel/src/fs/ext4/` - ext4 文件系统
- `kernel/src/fs/proc/` - procfs
- `userland/` - 用户态程序和库

---

## v0.4.0

**目标日期**: 2026-Q3

### 核心目标
**用户空间生态完善 + 可写文件系统**

### 必须完成（不可延期）

#### 1. 可写文件系统
- [ ] FAT32 写入（create/write/delete）
- [ ] ext4 写入（create/write/delete）
- [ ] 文件锁（flock/fcntl F_SETLK）
- [ ] 符号链接 / 硬链接

#### 2. 完整 syscall 子集（150+）
- [ ] 文件操作：openat(257) / rename(82) / unlink(87) / mkdir(83) / rmdir(84)
- [ ] 文件属性：chmod(90) / fchmod(91) / chown(92) / fchown(93)
- [ ] 时间：gettimeofday(96) / clock_gettime(228) / nanosleep(35)
- [ ] 内存：mprotect(10) / madvise(28) / msync(26) / mlock(149)
- [ ] 进程：setuid(105) / setgid(106) / setpgid(109) / setsid(112)
- [ ] 信号：rt_sigaction(13) / rt_sigprocmask(14) / rt_sigpending(127)

#### 3. ELF 加载完善
- [ ] 完整 PT_INTERP 支持
- [ ] TLS（Thread Local Storage）
- [ ] PIE/PIC 支持
- [ ] AT_* auxv 完整实现

#### 4. initramfs / initrd
- [ ] cpio 格式解析
- [ ] 启动时挂载根文件系统
- [ ] /init 进程启动

#### 5. 基础用户程序
- [ ] init（PID 1）
- [ ] sh（基础 shell）
- [ ] ls / cat / echo / mkdir / rm
- [ ] 测试工具（test runner）

### 验收标准
- [ ] 可创建/修改/删除文件
- [ ] 可运行 musl libc 编译的程序
- [ ] 可执行 shell 脚本
- [ ] `ls -la /` 正常工作
- [ ] syscall 覆盖率达到 150+

### 关键产出
- `kernel/src/fs/initramfs/` - initramfs 支持
- `userland/busybox/` - 基础工具集
- `userland/musl/` - musl libc 移植

---

## v0.5.0

**目标日期**: 2027-Q1

### 核心目标
**网络栈基础 + 可联网**

### 必须完成（不可延期）

#### 1. 网络驱动
- [ ] virtio-net 驱动
- [ ] e1000/e1000e 驱动（可选）
- [ ] 网络设备抽象（NetworkDevice trait）

#### 2. TCP/IP 协议栈
- [ ] 以太网层（ARP）
- [ ] IPv4（ICMP/UDP/TCP）
- [ ] IPv6（基础支持）
- [ ] socket 缓冲区管理
- [ ] 滑动窗口 + 拥塞控制（基础）

#### 3. Socket API
- [ ] socket(41) / bind(49) / listen(50) / accept(43)
- [ ] connect(42) / sendto(44) / recvfrom(45)
- [ ] sendmsg(46) / recvmsg(47)
- [ ] setsockopt(54) / getsockopt(55)
- [ ] getsockname(51) / getpeername(52)
- [ ] shutdown(48)

#### 4. 网络配置
- [ ] DHCP 客户端
- [ ] 静态 IP 配置
- [ ] DNS 解析（基础）

#### 5. 网络工具
- [ ] ifconfig / ip（基础）
- [ ] ping
- [ ] nc（netcat 最小实现）

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

## v0.6.0

**目标日期**: 2027-Q2

### 核心目标
**网络栈完善 + 高级特性**

### 必须完成（不可延期）

#### 1. TCP/IP 完善
- [ ] TCP 状态机完整实现
- [ ] Nagle 算法 / Delayed ACK
- [ ] TCP Fast Open
- [ ] Zero-copy sendfile

#### 2. 高级网络特性
- [ ] netlink socket（基础）
- [ ] unix domain socket
- [ ] packet socket（原始套接字）
- [ ] epoll(232) / epoll_ctl(233) / epoll_wait(232)
- [ ] eventfd(284)

#### 3. 网络文件系统
- [ ] NFS 客户端（基础，可选）

#### 4. 更多 syscall（200+）
- [ ] signalfd(282) / timerfd_create(283)
- [ ] inotify_init(253) / inotify_add_watch(254)
- [ ] prctl(157)
- [ ] sysinfo(99)
- [ ] uname(63)

### 验收标准
- [ ] HTTP 服务器可用（简单静态文件）
- [ ] SSH 客户端可用（dropbear）
- [ ] epoll 测试通过
- [ ] syscall 覆盖率达到 200+

---

## v0.7.0

**目标日期**: 2027-Q3

### 核心目标
**aarch64 架构移植**

### 必须完成（不可延期）

#### 1. aarch64 基础
- [ ] ARMv8-A 异常级别（EL0-EL3）
- [ ] 页表格式（4K/64K 页）
- [ ] GIC（Generic Interrupt Controller）
- [ ] ARM 定时器
- [ ] 串口（PL011）

#### 2. aarch64 内存管理
- [ ] 页表映射（TTBR0/TTBR1）
- [ ] TLB 管理
- [ ] 缓存一致性
- [ ] DMA 支持

#### 3. aarch64 进程/调度
- [ ] 上下文切换
- [ ] 系统调用（SVC 指令）
- [ ] 用户态切换（ERET）
- [ ] 信号帧布局

#### 4. aarch64 驱动
- [ ] virtio-blk/net（MMIO）
- [ ] SD/MMC（可选）
- [ ] GPIO（基础）

#### 5. aarch64 平台支持
- [ ] QEMU virt 机器
- [ ] 树莓派 4（基础启动，可选）

### 验收标准
- [ ] QEMU aarch64 启动成功
- [ ] v0.1-v0.6 功能在 aarch64 上全部可用
- [ ] 可执行 aarch64 用户程序
- [ ] `make run ARCH=aarch64` 正常工作

### 关键产出
- `kernel/src/arch/aarch64/` - aarch64 架构代码
- `boot/aarch64/` - aarch64 引导程序

---

## v0.8.0

**目标日期**: 2027-Q4

### 核心目标
**riscv64 架构移植**

### 必须完成（不可延期）

#### 1. riscv64 基础
- [ ] RISC-V 特权模式（U/S/M）
- [ ] 页表格式（Sv39/Sv48）
- [ ] PLIC（中断控制器）
- [ ] CLINT（核心本地中断器）
- [ ] RISC-V 定时器

#### 2. riscv64 内存管理
- [ ] 页表映射（satp CSR）
- [ ] SFENCE.VMA
- [ ] 缓存刷新

#### 3. riscv64 进程/调度
- [ ] 上下文切换
- [ ] 系统调用（ecall）
- [ ] 用户态切换（sret）
- [ ] 信号帧布局

#### 4. riscv64 驱动
- [ ] virtio-blk/net（MMIO）
- [ ] UART（ns16550a）
- [ ] HTIF（Host-Target Interface，可选）

#### 5. riscv64 平台支持
- [ ] QEMU virt 机器
- [ ] SiFive Unmatched（可选）

### 验收标准
- [ ] QEMU riscv64 启动成功
- [ ] v0.1-v0.6 功能在 riscv64 上全部可用
- [ ] 可执行 riscv64 用户程序
- [ ] `make run ARCH=riscv64` 正常工作

### 关键产出
- `kernel/src/arch/riscv64/` - riscv64 架构代码
- `boot/riscv64/` - riscv64 引导程序

---

## v0.9.0

**目标日期**: 2028-Q1

### 核心目标
**安全加固 + 性能优化**

### 必须完成（不可延期）

#### 1. 安全特性
- [ ] ASLR（地址空间随机化）
- [ ] NX/DEP（不可执行保护）
- [ ] Stack Canary（栈保护）
- [ ] RELRO（重定位只读）
- [ ] 用户/内核隔离强化

#### 2. 权限系统
- [ ] POSIX capabilities
- [ ] setuid/setgid 程序支持
- [ ] 基础审计日志

#### 3. 完整 syscall（250+）
- [ ] 完整信号系统
- [ ] 完整进程管理
- [ ] 完整文件系统操作
- [ ] 完整内存管理
- [ ] 完整网络操作
- [ ] ptrace(101)（基础调试支持）

#### 4. 性能优化
- [ ] 页表优化（大页支持）
- [ ] 调度器优化（CFS/EEVDF 基础）
- [ ] 文件系统缓存优化
- [ ] 网络零拷贝

#### 5. 稳定性
- [ ] 压力测试通过
- [ ] 长时间运行稳定性
- [ ] 内存泄漏检测
- [ ] 死锁检测

### 验收标准
- [ ] LTP（Linux Test Project）基础测试通过率 > 80%
- [ ] 可运行 gcc 编译简单程序
- [ ] 可运行 python 脚本
- [ ] 可运行 nginx 静态文件服务
- [ ] 三种架构全部通过回归测试

---

## v1.0.0 🎯

**目标日期**: 2028-Q2

### 核心目标
**正式发布**

### 必须完成（不可延期）

#### 1. Linux ABI 完整兼容
- [ ] 300+ 核心 syscall 实现
- [ ] LTP 测试通过率 > 95%
- [ ] 可运行主流 Linux 程序

#### 2. 三架构支持
- [ ] x86-64 生产就绪
- [ ] aarch64 生产就绪
- [ ] riscv64 生产就绪

#### 3. 完整驱动支持
- [ ] virtio 全家桶（blk/net/balloon/console）
- [ ] AHCI/NVMe（x86-64）
- [ ] e1000/e1000e（x86-64）
- [ ] 基础 USB（xHCI）

#### 4. 文档完备
- [ ] 内核 API 文档
- [ ] 驱动开发指南
- [ ] 用户手册
- [ ] 架构设计文档

#### 5. 工具链
- [ ] 完整构建系统
- [ ] 调试工具（gdb stub）
- [ ] 性能分析工具（基础）
- [ ] 系统监控工具

### 验收标准
- [ ] LTP 测试通过率 > 95%
- [ ] 可运行 systemd（基础）
- [ ] 可运行 gcc + make 构建项目
- [ ] 可运行 nginx + php-fpm
- [ ] 可运行 postgresql（基础）
- [ ] QEMU/VMware/真机启动
- [ ] 三种架构全部通过

### 发布物
- [ ] 源代码（GitHub）
- [ ] 预编译镜像（三种架构）
- [ ] SDK（交叉编译工具链）
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
recvfrom, recvmsg, recvmmsg, shutdown, setsockopt, getsockopt,
getsockopt
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
| 文件系统 | v0.3 | v0.7 | v0.8 |
| 网络 | v0.5 | v0.7 | v0.8 |
| 生产就绪 | v1.0 | v1.0 | v1.0 |

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 多架构工作量超预期 | 延期 v0.7/v0.8 | 优先保证 x86-64 稳定，架构代码充分抽象 |
| syscall 数量庞大 | 延期 v1.0 | 分版本逐步实现，优先高频 syscall |
| 性能不达标 | 影响可用性 | v0.9 专项性能优化，参考 Linux 调优 |
| 兼容性问题 | 用户程序运行失败 | 持续 LTP 测试，参考 glibc/strace |

---

## 里程碑总结

```
2026-Q1  v0.1 ✅  内核基础
2026-Q2  v0.2 ✅  进程/调度/IPC
2026-Q3  v0.3    文件系统/块设备
2026-Q4  v0.4    用户空间完善
2027-Q1  v0.5    网络栈基础
2027-Q2  v0.6    网络栈完善
2027-Q3  v0.7    aarch64 移植
2027-Q4  v0.8    riscv64 移植
2028-Q1  v0.9    安全加固
2028-Q2  v1.0    正式发布
```

---

## 下一步行动

当前进度：**v0.2 已完成，v0.3 进行中（Batch 1 已完成）**

下一步：v0.3 Batch 2（分区解析：MBR/GPT）与 Batch 3（VFS 核心抽象）
