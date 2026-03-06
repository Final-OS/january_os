# Source Map

## Primary Architecture Areas

- `boot/x86_64/`: UEFI bootloader implementation.
- `kernel/src/arch/x86_64/`: kernel-wide x86_64 primitives and interfaces.
- `kernel/src/interrupt/arch/x86_64/`: interrupt controller, IDT/GDT handlers.
- `kernel/src/mm/arch/x86_64/`: paging/TLB low-level memory mechanics.
- `kernel/src/smp/arch/x86_64/`: AP bootstrap and CPU bring-up.
- `kernel/src/task/arch/x86_64/`: task switching and user-mode entry glue.
- `tools/cfg/`: config generator for `os_cfg.toml`.

## Placement Rule

- Architecture-specific implementation code belongs in `boot/<arch>/` or `kernel/src/**/arch/<arch>/`, except `virt`, which uses `kernel/src/virt/platform/<isa>/` for subsystem-local virtualization backends.
- Generic module paths should only contain architecture-agnostic interfaces/policies.

## Documentation Ownership

- `docs/guide/`: user/developer workflow guides.
- `docs/api/`: module and API reference pages.
- `docs/implementation/`: deep implementation details.
- `docs/progress/`: roadmap and status.
- `docs/.vitepress/config.ts`: sidebar/nav routing source of truth.

## Skills Ownership

- `skills/january-os-project-intel/`: project context gathering process.
- `skills/january-os-docs-skills-sync/`: post-change docs/skills synchronization process.

## Build/Run Entry Points

- `Makefile`: canonical build/run/debug entrypoints.
- `os_cfg.toml`: runtime/build configuration source.

- `kernel/src/virt/platform/<isa>/`: virt subsystem platform virtualization backends (detect, VMX/SVM, EL2, H extension, irqchip/hypercall placeholders).
