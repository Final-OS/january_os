# boot/riscv64

`january_os` 的 RISC-V64 引导目录骨架。

当前状态：
- 提供独立 crate 入口（`january_os-boot-riscv64`）
- 仅保留最小 UEFI 入口占位

后续计划：
- 对齐 `boot/x86_64` 的阶段化结构（buffers/stages/handoff/paging）
- 补充 RISC-V64 启动页表与特权态切换交接逻辑
