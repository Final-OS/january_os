# Repository Guidelines

## Project Overview

january_os 是一个用 Rust 编写的 x86_64 操作系统，采用 UEFI 引导，目标是实现 Linux ABI 兼容。

**当前版本**: v0.2.0 ✅ → v0.3.0 🚧 (文件系统/块设备)

---

## Project Map

```
january_os/
│
├── boot/                          # 引导程序
│   ├── x86_64/                    # ✅ UEFI 引导 (uefi-rs, GOP)
│   ├── aarch64/                   # ⚠️ 脚手架，不可启动
│   └── riscv64/                   # ⚠️ 脚手架，不可启动
│
├── kernel/                        # 内核 crate (no_std)
│   └── src/
│       ├── main.rs                # 内核入口
│       ├── lib.rs                 # 内核库根
│       ├── init.rs                # 初始化编排
│       ├── generated/             # ⚠️ 自动生成，勿手改
│       │
│       ├── arch/x86_64/           # ✅ 架构原语 (页表、上下文切换)
│       │
│       ├── mm/                    # ✅ 内存管理
│       │   ├── api/               # 公共接口
│       │   ├── runtime/           # 运行时 (VMA, mm_struct)
│       │   ├── alloc/             # 分配器 (Buddy, SLUB, heap)
│       │   ├── boot/              # 启动期内存初始化
│       │   ├── virt/              # 虚拟内存 (paging, vmalloc)
│       │   └── arch/x86_64/       # 架构相关页表操作
│       │
│       ├── interrupt/             # ✅ 中断处理
│       │   ├── arch/x86_64/       # GDT, IDT, APIC, Timer
│       │   └── handlers/          # 异常/IRQ 处理
│       │
│       ├── task/                  # ✅ 进程/任务管理
│       │   ├── api/               # 公共接口
│       │   ├── proc/              # 进程管理 (fork, exec, wait, signal)
│       │   ├── thread/            # 线程管理
│       │   ├── sched/             # 调度器 (RR + work-stealing)
│       │   ├── syscall/           # 任务相关 syscall
│       │   └── arch/x86_64/       # 上下文切换, syscall 入口
│       │
│       ├── fs/                    # 🚧 文件系统
│       │   ├── api/               # VFS trait 定义
│       │   ├── runtime/           # 运行时 (fd 表, mount 管理)
│       │   ├── vfs/               # VFS 核心 (path 解析, mount)
│       │   ├── fd/                # 文件描述符桥接
│       │   ├── backing/           # 文件系统后端
│       │   │   ├── initramfs/     # ✅ 启动根文件系统
│       │   │   ├── fat/           # ✅ FAT32 只读
│       │   │   ├── ext4/          # ✅ ext4 只读
│       │   │   └── mmap/          # mmap 后端
│       │   └── syscall/           # 文件 syscall
│       │
│       ├── drivers/               # 🚧 设备驱动
│       │   ├── block/             # 块设备 (virtio-blk, MBR/GPT)
│       │   ├── input/             # 键盘/鼠标
│       │   ├── serial/            # UART
│       │   ├── tty/               # TTY 子系统
│       │   ├── usb/               # xHCI + HID
│       │   └── acpi/              # ACPI 表解析
│       │
│       ├── syscall/               # ✅ 系统调用框架
│       │   ├── abi/               # ABI 定义
│       │   ├── dispatch/          # 分发逻辑
│       │   └── table/             # syscall 表
│       │
│       ├── sync/                  # ✅ 同步原语
│       │   ├── spinlock.rs
│       │   ├── mutex.rs
│       │   ├── semaphore.rs
│       │   └── rcu.rs
│       │
│       ├── libs/                  # ✅ 内核数据结构
│       │   ├── rbtree.rs          # 红黑树
│       │   ├── btree.rs           # B-Tree
│       │   ├── mptree.rs          # Maple Tree (VMA)
│       │   ├── rdtree.rs          # Radix Tree
│       │   ├── lru.rs             # LRU 缓存
│       │   └── rcu.rs             # RCU 机制
│       │
│       ├── smp/                   # ✅ SMP 多核
│       │   └── arch/x86_64/       # AP 启动, IPI
│       │
│       ├── net/                   # ⚠️ 网络栈骨架
│       ├── security/              # ⚠️ 安全子系统骨架
│       ├── virt/                  # ⚠️ 虚拟化骨架
│       │
│       ├── shell/                 # 内核 shell
│       └── tests/                 # 内核测试
│           ├── mm/
│           ├── task/
│           ├── fs/
│           ├── libs/
│           └── ...
│
├── tools/
│   ├── cfg/                       # 配置生成器 (os_cfg.toml → Rust)
│   ├── mkinitramfs/               # initramfs 打包工具
│   └── vmcfg/                     # VM 配置
│
├── userland/                      # 用户态程序
│   ├── init/                      # PID 1 入口
│   ├── sh/                        # 基础 shell
│   ├── ls/, cat/, echo/, pwd/     # 基础工具
│   └── runtime/                   # 用户态运行时
│
├── initramfs/                     # initramfs 根文件系统模板
│   ├── bin/
│   ├── dev/
│   ├── etc/
│   └── ...
│
├── docs/                          # 文档站点 (VitePress)
│   ├── guide/                     # 用户指南
│   ├── api/                       # API 参考
│   ├── implementation/            # 实现详解
│   └── progress/                  # 开发进度
│
├── skills/                        # AI Skills (Claude Code)
│   └── ...
│
├── os_cfg.toml                    # 系统配置
├── Makefile                       # 构建系统
└── Cargo.toml                     # Workspace 根
```

### 图例

| 符号 | 含义 |
|------|------|
| ✅ | 功能完整，可用于生产 |
| 🚧 | 开发中，部分功能可用 |
| ⚠️ | 脚手架/占位，不可用于运行时 |

---

## Documentation Locations

### 文档站点结构 (`docs/`)

| 目录 | 用途 | 关键文件 |
|------|------|----------|
| `docs/guide/` | 用户/开发者指南 | `overview.md`, `configuration.md`, `skills-info-flow.md` |
| `docs/api/` | API 参考 | `overview.md`, `mm/*.md`, `task/*.md`, `fs/*.md`, `sync/*.md` |
| `docs/implementation/` | 实现详解 | `overview.md`, `boot.md`, `memory-init.md`, `gdt.md`, `idt.md` |
| `docs/progress/` | 开发进度 | `overview.md`, `roadmap.md`, `tech-debt.md`, `v0.3-plan.md` |

### 核心文档速查

| 文档 | 路径 | 说明 |
|------|------|------|
| 项目指南 | `AGENTS.md` | 本文件，项目结构与规范 |
| Claude 指南 | `CLAUDE.md` | Claude Code 工作指南 |
| 开发进度 | `docs/progress/overview.md` | 当前状态与已完成功能 |
| 完整路线图 | `docs/progress/roadmap.md` | v0.1 → v1.0 版本规划 |
| 技术债务 | `docs/progress/tech-debt.md` | 债务清单与偿还计划 |
| v0.3 计划 | `docs/progress/v0.3-plan.md` | 当前版本分批实施计划 |
| 配置说明 | `docs/guide/configuration.md` | os_cfg.toml 配置详解 |
| API 索引 | `docs/api/overview.md` | API 文档入口 |

### Skills 文档 (`skills/`)

| Skill | 用途 |
|-------|------|
| `january-os-project-intel/` | 项目上下文收集 |
| `january-os-docs-skills-sync/` | 文档同步 |
| `january-os-architecture-planner/` | 架构规划 |

---

## Module Organization Summary

- `boot/x86_64/`: UEFI bootloader crate for the active runtime path.
- `boot/aarch64/`, `boot/riscv64/`: scaffold bootloader crates kept to preserve multi-arch layout; they are not bootable yet.
- `kernel/`: standalone `no_std` kernel crate; core areas include `arch/`, `drivers/`, `interrupt/`, `mm/`, `task/`, and `sync/`.
- `tools/cfg/`: configuration tool that reads `os_cfg.toml` and generates Rust config code.
- `docs/`: VitePress documentation site; `target/` stores build outputs.
- `kernel/src/generated/` is generated by `make build-kernel`; do not hand-edit generated files.

## Build, Test, and Development Commands
- `make install-deps`: install Rust target/components and list required system packages.
- `make build`: build bootloader + kernel and prepare EFI layout.
- `make run` / `make run-gui`: run in QEMU (serial or GUI console).
- `make debug`: start QEMU with GDB server on `:1234`.
- `make iso`: generate `target/january_os.iso` for VM/real hardware boot.
- `make clean` / `make config` / `make help`: clean artifacts, print config, list targets.
- Docs workflow: `cd docs && pnpm install && pnpm dev` (or `pnpm build`).

## Coding Style & Naming Conventions
- Follow idiomatic Rust style with `rustfmt` defaults (4-space indentation, trailing commas where useful).
- Use `snake_case` for modules/functions, `CamelCase` for types/traits, and `SCREAMING_SNAKE_CASE` for constants.
- Keep architecture-specific code under `boot/<arch>/`, `kernel/src/**/arch/<arch>/`, or `kernel/src/virt/platform/<isa>/` for virtualization backends; avoid mixing generic and arch code.
- Keep test/demo-only logic out of runtime kernel paths (`kernel/src/**` except `kernel/src/tests/**`).
- Avoid `demo` / `test` wording in default runtime kernel command names, paths, constants, and logs.
- Keep changes minimal and localized; prefer extending existing modules over creating parallel patterns.

## Testing Guidelines
- There is no dedicated automated kernel test suite yet; validate changes with:
  - `make build`
  - `make run` (or `make run-gui`)
- Put kernel test code and test assets under `kernel/src/tests/**` only.
- Organize `kernel/src/tests/**` by subsystem directories (for example `tests/mm/`, `tests/task/`, `tests/libs/`) instead of long-term flat layout.
- tests/demo scenarios must be functionally complete: cover main path, key branches, failure paths, and recovery paths.
- tests/demo logs must be step-level and actionable: include step action, input, expected result, actual result, and failure location.
- tests/demo coverage must include invalid input, unexpected input, and boundary-condition cases; do not only test happy-path behavior.
- For config-related changes, run `make config` and verify regenerated files under `kernel/src/generated/`.
- For docs-only changes, run `cd docs && pnpm build` before opening a PR.

## Commit & Pull Request Guidelines
- Existing history favors short, imperative commit subjects (for example, `add tsc`, `update run-gui`).
- Preferred commit format: `<area>: <imperative summary>` (example: `mm: fix buddy allocator split`).
- PRs should include: purpose, affected paths, validation steps/commands, and any `os_cfg.toml` impact.
- For runtime-visible changes, include relevant boot logs, serial output, or screenshots.
