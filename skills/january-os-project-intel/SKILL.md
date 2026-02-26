---
name: january-os-project-intel
description: Build a complete, current project briefing for january_os before implementation or review. Use when users ask for architecture status, impact analysis, onboarding context, or stale docs/skills checks.
---

# January OS Project Intel

## Overview

Use this skill to produce an up-to-date technical briefing of `january_os` with architecture, build/runtime context, change scope, and docs/skills coverage in one pass.

## When To Use

- User asks for project overview, module explanation, or architecture map.
- User asks impact analysis before modifying code.
- User asks to review whether docs/skills are in sync with current implementation.
- You are about to start non-trivial code changes and need a reliable baseline.

## Required Output

Return a concise Intel Report with these sections:
- Repository State: branch, dirty files, active build targets.
- Module Map: boot/kernel/tools/docs ownership and arch split.
- Change Impact: touched paths, likely subsystems, runtime risk points.
- Docs+Skills Coverage: what docs/skills files already cover the area and what is missing.
- Action Plan: concrete next edits and verification commands.

## Workflow

1. Collect baseline context.
Run `scripts/collect_project_intel.sh` from repo root to capture current status snapshot.

2. Read authoritative project files.
Always inspect at least:
- `AGENTS.md`
- `Makefile`
- `os_cfg.toml`
- `docs/.vitepress/config.ts`
- `kernel/src/arch/`, `kernel/src/*/arch/`

3. Build a scope-specific subsystem map.
Use `references/source-map.md` and `references/intel-checklist.md` to connect changed paths to subsystem responsibilities.

4. Run minimum verification signal.
If code/build-related context changed, run at least one relevant command (`make build`, `make config`, or docs build for docs-only changes).

5. Emit an Intel Report that includes docs+skills sync recommendations.
If there are edits, hand off to `$january-os-docs-skills-sync`.

## Constraints

- Prefer facts from repository files and command outputs over assumptions.
- If info is missing, mark uncertainty explicitly and propose the next file/command to resolve it.
- Keep the report structured and actionable; avoid narrative-only summaries.
- Flag any API/behavior change that is not reflected in `docs/api/**`.
- Flag any architecture-specific logic implemented outside `boot/<arch>/` or `kernel/src/**/arch/<arch>/`, and require either relocation or explicit temporary rationale.

## Resources

- `scripts/collect_project_intel.sh`: quick repository snapshot generator.
- `references/intel-checklist.md`: mandatory checklist and command matrix.
- `references/source-map.md`: subsystem source-of-truth map.
