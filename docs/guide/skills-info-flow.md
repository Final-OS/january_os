# Skills 与信息流维护

本项目在仓库根目录提供了专用技能目录 `skills/`，用于保证“代码实现、文档、技能说明”三者同步。

## 技能列表

- `skills/january-os-project-intel`
  - 用途：快速拉齐当前项目全貌（架构、构建入口、变更影响、文档覆盖）。
- `skills/january-os-docs-skills-sync`
  - 用途：每次改动后，强制执行 docs/skills 同步更新流程。
- `skills/january-os-architecture-planner`
  - 用途：输出“当前架构 + 设计图 + 阶段计划 + 子系统细分”的统一规划文档。

## 变更后的必做动作

1. 收集变更范围（`git status --short`）。
2. 根据路径映射补齐 docs 更新（实现/API/指南/进度）。
3. 若工作流或结构发生变化，同步更新 `skills/` 下相关 `SKILL.md` 与 `references/`。
3.1 测试与运行时代码隔离：测试/演示代码与资源仅放在 `kernel/src/tests/**`，并按子系统分目录管理，避免平铺。
4. 进行最小验证：
   - 代码相关：`make build`
   - 文档相关：`cd docs && pnpm build`
5. 在最终说明中明确列出：
   - 改动文件
   - 对应 docs 更新
   - 对应 skills 更新
   - 验证结果

## 路径映射规则

完整映射见：
- `skills/january-os-docs-skills-sync/references/update-matrix.md`

建议在进行中大型改动前，先运行：
- `skills/january-os-project-intel/scripts/collect_project_intel.sh`

它可快速生成仓库状态快照，便于后续影响分析与信息同步。
