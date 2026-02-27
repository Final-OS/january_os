# boot/aarch64

`january_os` 的 AArch64 引导目录骨架。

当前状态：
- 提供独立 crate 入口（`january_os-boot-aarch64`）
- 仅保留最小 UEFI 入口占位

后续计划：
- 对齐 `boot/x86_64` 的阶段化结构（buffers/stages/handoff/paging）
- 补充 AArch64 页表与异常级切换交接逻辑
