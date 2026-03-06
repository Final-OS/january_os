# Security API

`security` 子系统已按组件化宏内核规范重组为 façade + 分层子目录骨架。

## 目录结构

```text
kernel/src/security/
├── mod.rs
├── error.rs
├── api/
├── cred/
├── policy/
├── hook/
├── audit/
├── runtime/
├── syscall/
└── diag/
```

## 顶层 façade

- `security::init_early()` / `security::init_core()` / `security::init_late()`：组件生命周期入口
- `security::component_stats()` / `security::dump_state()`：运行态诊断入口
- `security::syscall::dispatch()`：安全域 syscall 占位分发入口

## 子域职责

- `api/`：`SecurityAction`、`Capability`、`PolicyDecision` 与请求结构
- `cred/`：UID/GID、凭据快照、任务凭据挂接占位
- `policy/`：默认策略提供者、策略引擎、规则集占位
- `hook/`：`fs/net/task` 安全检查落点与 LSM 风格 hook trait
- `audit/`：审计事件、ring buffer、sink 占位
- `runtime/`：初始化、管理器、注册表、组件状态
- `syscall/`：安全域 syscall 子命令与统一 `ENOSYS` 占位语义
- `diag/`：`dump_state()` 与 `component_stats()` 聚合

## 当前语义

- `init_early()` 和 `init_core()` 返回成功，用于完成组件编排闭环
- `init_late()` 默认返回 `KernelError::NotSupported`
- policy/hook/audit/syscall 默认维持 `Defer` 或 `Unsupported/ENOSYS`
- 该骨架为后续 `UID/GID + DAC + capability + LSM + audit runtime` 实装提供稳定目录边界
