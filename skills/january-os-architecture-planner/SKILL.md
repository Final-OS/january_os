---
name: january-os-architecture-planner
description: Build a current architecture design and execution plan for january_os, including system diagrams, phased roadmap, and subsystem-level breakdown. Use when users ask for overall planning, technical blueprint, or implementation sequencing.
---

# January OS Architecture Planner

## Overview

Use this skill to produce a grounded architecture blueprint for `january_os` from current repository facts.  
Deliverables must include:
- Overall design (what exists now)
- Architecture diagram(s)
- Phased plan (what to build next)
- Local subsystem breakdown with acceptance criteria

## When To Use

- User asks for overall system design or architecture planning.
- User asks for roadmap, milestones, or phase sequencing.
- User asks for a "design diagram" or layered architecture figure.
- User asks to split planning into global and subsystem-level tasks.

## Required Output

Return a concise architecture report with these sections:
- Baseline Snapshot: current implementation status and scope.
- Multi-Arch Plan: x86_64 / aarch64 / riscv64 split and shared interfaces.
- Virtualization Plan: guest-first capabilities and host-side roadmap.
- Design Diagram: boot-to-kernel and subsystem layering.
- Overall Plan: versioned goals and execution stages.
- Local Breakdown: per-subsystem status, next work, acceptance signal.
- Risks & Dependencies: critical blockers and coupling points.

## Workflow

1. Collect authoritative context.
- Read: `AGENTS.md`, `README.md`, `Makefile`, `os_cfg.toml`.
- Read: `kernel/src/main.rs`, `kernel/src/init.rs`, `boot/x86_64/src/main.rs`.
- Read progress docs: `docs/progress/overview.md`, `docs/progress/v0.2-plan.md`.

2. Build current-state map.
- Confirm implemented subsystems from code and init order:
  - Boot handoff
  - Memory
  - Interrupt/APIC
  - ACPI/IOMMU
  - SMP
  - Task/Scheduler/Syscall
  - Drivers
- Mark empty placeholders explicitly (`fs`, `net`, `security`).
- Mark per-architecture readiness explicitly (`x86_64`, `aarch64`, `riscv64`).

3. Produce diagram and staged plan.
- Draw at least one architecture diagram in ASCII.
- Tie plan stages to existing version plan (`v0.2`, `v0.3`, ...).
- Keep plan executable with concrete acceptance criteria.

4. If changes are written to docs/skills, sync site navigation.
- Update `docs/.vitepress/config.ts` if new docs page added.
- Update `docs/guide/skills-info-flow.md` if skill inventory changes.

5. Validate minimum signal.
- For docs-only output: run `cd docs && pnpm build`.
- If build cannot run, report why and what remains unverified.

## Constraints

- Prefer repository facts over assumptions.
- Include exact file paths in conclusions.
- Distinguish "already implemented" vs "planned".
- Use the fixed architecture baseline: modular monolithic kernel + componentized OS.
- Keep target architecture baseline explicit: x86_64 + aarch64 + riscv64.
- Include virtualization split: guest support first, host capabilities staged.
- Keep architecture-specific placement rules:
  - `boot/<arch>/`
  - `kernel/src/**/arch/<arch>/`
