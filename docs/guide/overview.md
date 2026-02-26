# 指南

欢迎使用 january_os 文档。

## 文档导航

| 文档 | 说明 |
|------|------|
| [配置说明](./configuration.md) | os_cfg.toml 配置项详解 |
| [Skills 与信息流](./skills-info-flow.md) | 变更后 docs/skills 同步维护流程 |
| [API 参考](../api/overview.md) | 完整的 API 文档 |
| [实现详解](../implementation/overview.md) | 内部实现细节 |
| [开发进度](../progress/overview.md) | 当前开发状态 |

## 快速开始

安装 rust 工具链

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup install nightly
rustup default nightly
rustup component add rust-src llvm-tools-preview
cargo install cargo-binutils
```

```bash
# 安装依赖
make install-deps

# 构建
make build

# 运行
make run

# 创建 ISO 镜像
make iso

# 清理
make clean

# 显示配置
make config

# 显示帮助
make help
```

详细配置请参阅 [配置说明](./configuration.md)。
