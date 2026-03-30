# january_os Agent Guide

This file is for agentic coding tools working in `january_os`.

## Project Summary

- `january_os` is a Rust `x86_64` operating system with UEFI boot.
- The active runtime path is `boot/x86_64/` + `kernel/`.
- `boot/aarch64/` and `boot/riscv64/` exist as scaffolding and are not bootable.
- The current development focus is the `v0.3.x` filesystem / block-device follow-up work, with `v0.3.1` items largely landed and `v0.3.2` next.
- The kernel is `no_std`, uses custom targets, and is not a normal host-side Rust app.

## High-Level Layout

- `boot/<arch>/`: bootloader crates.
- `kernel/`: standalone kernel crate.
- `userland/`: small userspace programs bundled into initramfs.
- `tools/cfg/`: config generator for `os_cfg.toml`.
- `tools/mkinitramfs/`: initramfs builder.
- `tools/vmcfg/`: VM config helper.
- `initramfs/`: root filesystem template.
- `docs/`: VitePress docs site.
- `kernel/src/generated/`: generated; do not hand-edit.

## Architecture Rules

- Keep architecture-specific code under `boot/<arch>/`, `kernel/src/arch/<arch>/`, or `kernel/src/**/arch/<arch>/`.
- Do not mix generic subsystem logic with `x86_64`-specific implementation details.
- Prefer extending an existing subsystem over creating a parallel abstraction.
- Respect the project’s componentized monolithic kernel direction.

## Component Boundaries

- Preferred dependency direction is:
- `arch` / `libs` / `sync`
- → `mm` / `interrupt`
- → `task`
- → `fs` / `drivers` / `virt`
- → `syscall`
- → `shell` / `tests`
- Lower layers should not depend on higher layers.
- `syscall` should orchestrate component APIs, not own core resource management.
- `tests` may reach across layers, but test helpers must not leak into runtime code.

## Build Commands

- `make install-deps` — install Rust targets/components and required system packages.
- `make build` — build bootloader, kernel, userland, and initramfs.
- `make build-tools` — build helper tools only.
- `make build-kernel` — build only the kernel image.
- `make build-boot` — build only the bootloader.
- `make build-userland` — build only userland binaries.
- `make prepare-initramfs` — stage and pack initramfs.
- `make iso` — create `target/january_os.iso`.
- `make clean` — remove build artifacts.
- `make config` — print effective kernel config from `os_cfg.toml`.
- `make vm-config` — print effective VM config from `vm_cfg.toml`.

## Run Commands

- `make run` — boot in QEMU with serial console.
- `make run-gui` — boot in QEMU with GUI console.
- `make run-ksh` — boot directly into the kernel shell.
- `make run-gui-ksh` — GUI boot directly into the kernel shell.
- `make run-scsi` — boot in QEMU with an additional `virtio-scsi` disk on LUN0.
- `make run-scsi-ksh` — boot directly into the kernel shell with an additional `virtio-scsi` disk.
- `make debug` — boot paused with GDB server on `:1234`.
- `make debug-ksh` — debug boot directly into the kernel shell.
- `make debug-scsi` — debug boot paused with an additional `virtio-scsi` disk.
- `make debug-scsi-ksh` — debug boot directly into the kernel shell with an additional `virtio-scsi` disk.

## Test Reality

- There is no conventional host-side `cargo test` workflow for the kernel.
- Kernel tests run inside the booted OS through the kernel shell command `test`.
- For most code changes, validation means `make build` and then running targeted in-VM tests.
- For config-related changes, also run `make config` and inspect regenerated files under `kernel/src/generated/`.

## Single Test Workflow

- Start the OS in the kernel shell with `make run-ksh`.
- At the `ksh` prompt, run a targeted test command.
- Prefer the smallest relevant test first before broader subsystem coverage.
- Examples:
- `test libs rbtree`
- `test libs rcu`
- `test mm buddy`
- `test mm mmap`
- `test task switch`
- `test task usermode`
- `test fs path`
- `test fs ext4`
- `test block virtio`
- `test vfs mount`

## Test Command Matrix

- `test task [name]` — available: `switch`, `wait`, `usermode`, `regression`, `safe`, `all`.
- `test libs [name]` — available: `rbtree`, `lru`, `rdtree`, `btree`, `mptree`, `rcu`, `ring_buffer`, `kfifo`, `bitmap`, `hlist`, `wait_queue`, `id_allocator`, `sync_once`, `sync_blocking`.
- `test mm [name]` — available: `swiotlb`, `dma_coherent_guard`, `slub`, `buddy`, `page_counter_guard`, `status_readonly`, `pcp`, `heap`, `mmap`, `pt_ownership`, `pt_reclaim`, `vmalloc_heal`.
- `test smp [name]` — available: `topology`, `cpu_id`, `ipi`, `irq_route`, `sched_stats`, `all`.
- `test fs [name]` — available: `path`, `mount`, `fd_bridge`, `fat32`, `ext4`.
- `test block [name]` — available: `virtio`, `partition`.
- `test vfs [name]` — available: `path`, `mount`, `fd_bridge`.
- `test all` — run the full in-kernel test sweep.

## Recommended Validation Order

- For Rust code changes: run `make build` first.
- For kernel subsystem changes: run `make run-ksh` and the smallest matching `test ...` command.
- For boot/runtime-visible changes: also do a normal `make run` smoke test.
- For docs-only changes: run `cd docs && pnpm build`.

## Format and Lint Commands

- There is no dedicated lint target in `Makefile`.
- Use `rustfmt` defaults; no project `rustfmt.toml` was found.
- Workspace formatting: `cargo fmt --all`.
- Kernel formatting: `cargo fmt --manifest-path kernel/Cargo.toml --all`.
- Docs formatting/linting is not separately configured; `pnpm build` is the main docs validation step.
- Clippy is not strongly enforced today; `kernel/src/main.rs` currently allows all Clippy lints.
- If you run Clippy, treat it as advisory unless the task is specifically lint cleanup.

## Docs Commands

- `cd docs && pnpm install` — install docs dependencies.
- `cd docs && pnpm dev` — run the VitePress dev server.
- `cd docs && pnpm build` — build the docs site.
- `cd docs && pnpm preview` — preview the built docs.

## External Agent Rules

- No `.cursor/rules/**` directory was found.
- No `.cursorrules` file was found.
- No `.github/copilot-instructions.md` file was found.
- If any of those files appear later, treat them as additional agent instructions and merge them with this guide.

## Rust Style

- Follow idiomatic Rust and keep changes minimal.
- Use `rustfmt` defaults and preserve existing formatting style in touched files.
- Prefer small, focused functions over large rewrites.
- Keep public APIs narrow; expose façades instead of deep internals when possible.
- Avoid introducing new crates unless clearly necessary.

## Imports

- Match the local file’s import style instead of forcing a new pattern.
- In kernel code, imports commonly group as `crate::...`, then `alloc::...`, then `core::...`, but consistency within the file matters most.
- Remove unused imports when touching a file unless the file intentionally tolerates them during bring-up.
- Prefer explicit imports over wildcard imports, except where the module already uses local `super::*` patterns.

## Naming

- Modules and functions: `snake_case`.
- Types and traits: `CamelCase`.
- Constants and statics: `SCREAMING_SNAKE_CASE`.
- Keep runtime-visible names production-oriented; avoid `demo` / `test` wording in non-test runtime paths, commands, constants, and logs.

## Types and APIs

- Prefer concrete, explicit types at subsystem boundaries.
- Use `Result<T, E>` / project aliases for fallible operations.
- Use `Option<T>` only when absence is a normal, non-diagnostic state.
- Prefer small structs/enums to passing long primitive parameter lists across components.
- For cross-component interfaces, prefer stable descriptors, reports, and context structs.

## Error Handling

- Use existing errno-style or subsystem-specific result conventions already present in the touched area.
- In kernel-wide code, `KernelError` / `KernelResult<T>` may be appropriate where that pattern already exists.
- In syscall-facing code, return encoded errno values through the established syscall helpers.
- Fail early on invalid input and preserve existing error semantics.
- Do not swallow errors silently; log only when the surrounding code already logs similar failures.

## Unsafe Code

- Keep `unsafe` blocks as small and local as possible.
- Document the safety rationale for non-obvious `unsafe` code.
- Prefer safe wrappers around repeated unsafe operations.
- Do not expand unsafe surface area unless required by the task.

## Concurrency and State

- Use existing synchronization primitives from `kernel/src/sync/`.
- Avoid exposing raw global mutable state across component boundaries.
- Prefer explicit state/stat reporting functions for diagnostics.
- Preserve interrupt, scheduler, and memory-ordering assumptions in low-level code.

## Logging and Diagnostics

- Follow existing logging style such as `kprintln!`, `warn!`, and subsystem-tagged messages.
- Test logs should be step-level and actionable: include action, input, expected result, actual result, and failure location.
- Do not add noisy logs to hot paths unless the code already gates them behind a debug flag.

## Testing Code Placement

- Keep kernel tests and test assets under `kernel/src/tests/**` only.
- Organize tests by subsystem, not as a long flat list.
- Do not leave test-only helpers in runtime modules after the task is done.

## Generated and Config Files

- Do not hand-edit `kernel/src/generated/`.
- Source-of-truth config lives in `os_cfg.toml` and `vm_cfg.toml`.
- If config changes affect generated output, validate with `make config` and a rebuild.

## Docs and Technical Debt

- Update docs when behavior, layout, commands, or architecture expectations change.
- Keep docs aligned with actual code, not aspirational design.
- If a change only partially completes a larger architecture goal, note follow-up debt in the docs the repo already uses for progress tracking.

## Agent Working Style

- Prefer surgical changes over broad refactors.
- Check for generated code and arch-specific boundaries before editing.
- Validate the smallest relevant scope first.
- If you touch user-visible behavior, mention the exact command the next agent or human should run to verify it.
