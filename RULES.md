# Repository Rules (january_os)

本文件用于约束在本仓库中的实现、文档同步与交付行为。规则来源：
- `AGENTS.md`
- `docs/guide/skills-info-flow.md`
- `skills/january-os-docs-skills-sync/**`
- `skills/january-os-project-intel/**`
- `skills/january-os-architecture-planner/**`

## 1. 适用范围

- 任何对 `boot/`、`kernel/`、`tools/`、`os_cfg.toml`、`Makefile`、`docs/`、`skills/` 的改动。
- 任何 API、行为、架构、构建流程、配置语义变更。

## 2. 不可协商规则

- 每次有意义的代码/配置变更，必须做 `docs/skills` 影响判断。
- 任何 API 或外部可观察行为变更，必须在同一变更中更新 `docs/api/**`。
- 任何架构相关行为变更，必须在同一变更中更新 `docs/api/arch/**` 和相关 `docs/implementation/**`。
- 新增或重命名 docs 页面时，必须同步更新 `docs/.vitepress/config.ts`。
- 若技能工作流、假设或命令变更，必须同步更新对应 `skills/*/SKILL.md` 与 `skills/*/agents/openai.yaml`。
- 架构专用代码必须放在架构目录：
  - `boot/<arch>/`
  - `kernel/src/**/arch/<arch>/`
- 禁止手改生成文件：`kernel/src/generated/**`（应通过 `make build-kernel` 或 `cfg generate` 生成）。
- 若最终判断不需要更新 docs/skills，必须在交付说明中给出明确理由。

## 3. 变更映射规则（Path -> 必查更新）

- `boot/**`
  - `docs/implementation/boot.md`
  - `docs/api/arch/x86_64.md`（如导出行为变化）
- `kernel/src/arch/**`, `kernel/src/*/arch/**`
  - `docs/api/arch/x86_64.md`
  - 相关 `docs/api/**` 与 `docs/implementation/**`
- `kernel/src/mm/**`
  - 相关 `docs/api/mm/*.md`
  - `docs/implementation/memory-init.md` 或 `docs/implementation/iommu.md`
- `kernel/src/interrupt/**`
  - `docs/api/interrupt/*.md`
  - `docs/implementation/gdt.md`
  - `docs/implementation/idt.md`
  - `docs/implementation/apic.md`
- `kernel/src/drivers/**`
  - 相关 `docs/api/drivers/*.md`
  - 相关 `docs/implementation/*.md`
- `kernel/src/task/**`, `kernel/src/syscall/**`, `kernel/src/sync/**`
  - 相关 `docs/api/task/`, `docs/api/syscall/`, `docs/api/sync/`
- `tools/cfg/**`, `os_cfg.toml`
  - `docs/implementation/cfg-tool.md`
  - `docs/guide/configuration.md`
- `Makefile`, `Cargo.toml`, `rust-toolchain.toml`
  - `docs/guide/overview.md`
  - 相关实现文档与 skills 引用命令
- `docs/**`
  - `docs/.vitepress/config.ts`
  - 必要时同步 `skills/**` 引用路径
- `skills/**`
  - 保持 `SKILL.md` 与 `agents/openai.yaml` 一致
  - 若共享假设变化，同步对应技能

## 4. 执行流程

1. 收集上下文
- `git status --short`
- `git diff --name-only`
- 必要时读取 `AGENTS.md`、`Makefile`、`os_cfg.toml`、`docs/.vitepress/config.ts`

2. 实施最小改动
- 仅改与任务相关文件，避免无关重构。
- 优先复用现有模块与模式，不平行造新体系。

3. 同步 docs/skills
- 按第 3 节映射规则逐项检查并更新。

4. 最小验证
- 代码/构建相关：`make build`（至少其一：`make build` 或 `make config`）
- docs 相关：`cd docs && pnpm build`

5. 交付报告
- 必须列出：
  - 改动文件
  - docs 更新文件
  - skills 更新文件
  - 验证命令与结果
  - 未覆盖项与原因

## 5. 输出质量要求

- 所有结论必须可追溯到文件内容或命令输出。
- 给出精确路径，不用模糊描述。
- 明确区分“已实现”与“计划项”。
- 有不确定项时，明确缺口与下一步验证命令。
