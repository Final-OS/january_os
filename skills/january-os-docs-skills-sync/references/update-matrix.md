# Update Matrix

## Core Rule

If a file in the left column changes, evaluate and update every target in the right column unless explicitly not applicable.

## Guardrails

- API or externally visible behavior changes require same-change updates to `docs/api/**`.
- Architecture-specific code changes require same-change updates to `docs/api/arch/**` and relevant `docs/implementation/**`.
- Architecture-specific code should be placed under `boot/<arch>/` or `kernel/src/**/arch/<arch>/`; if not, add refactor item and document temporary rationale.
- Runtime kernel code (`kernel/src/**` except `kernel/src/tests/**`) should not carry test/demo-only code, assets, or naming.
- Kernel tests should reside under `kernel/src/tests/**` and be grouped by subsystem directories.

## Path -> Required Updates

- `boot/**`
  - `docs/implementation/boot.md`
  - `docs/api/arch/x86_64.md` (if exported behavior changed)
  - Related skill references if workflow or build assumptions changed

- `kernel/src/arch/**`, `kernel/src/*/arch/**`
  - `docs/api/arch/x86_64.md`
  - Related subsystem API page under `docs/api/**`
  - Related implementation page under `docs/implementation/**`

- `kernel/src/mm/**`
  - `docs/api/mm/*.md` impacted pages
  - `docs/implementation/memory-init.md` or `docs/implementation/iommu.md`

- `kernel/src/interrupt/**`
  - `docs/api/interrupt/*.md`
  - `docs/implementation/gdt.md`
  - `docs/implementation/idt.md`
  - `docs/implementation/apic.md`

- `kernel/src/drivers/**`
  - impacted `docs/api/drivers/*.md`
  - relevant `docs/implementation/*.md` pages (`acpi.md`, `tty.md`, etc.)

- `kernel/src/task/**`, `kernel/src/syscall/**`, `kernel/src/sync/**`
  - impacted pages under `docs/api/task/`, `docs/api/syscall/`, `docs/api/sync/`

- `kernel/src/tests/**`
  - keep tests organized by subsystem folders (`tests/mm/`, `tests/task/`, `tests/libs/`, etc.)
  - keep test assets under matching test subtree (for example `tests/task/assets/`)
  - if test-only path is promoted to runtime path, require explicit rationale and follow-up refactor item

- `tools/cfg/**`, `os_cfg.toml`
  - `docs/implementation/cfg-tool.md`
  - `docs/guide/configuration.md`

- `Makefile`, `Cargo.toml`, `rust-toolchain.toml`
  - `docs/guide/overview.md`
  - relevant implementation notes if build flow changes
  - skill references mentioning commands/verification flow

- `docs/**`
  - `docs/.vitepress/config.ts` if page list, titles, or structure changed
  - skills references if canonical paths or workflow changed

- `skills/**`
  - keep `SKILL.md` and `agents/openai.yaml` aligned
  - update counterpart skill when shared workflow assumptions changed

## Nav/Sidebar Rule

For every new docs page:
- Add sidebar entry in `docs/.vitepress/config.ts`.
- Add cross-link from an existing overview page if discoverability would otherwise be poor.
