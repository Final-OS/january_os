---
name: january-os-docs-skills-sync
description: Synchronize code/config changes into docs and skills in january_os. Use after every meaningful change to keep information flow complete and current.
---

# January OS Docs Skills Sync

## Overview

This skill enforces post-change information hygiene. It maps changed code/config/build files to required updates in `docs/` and `skills/`, then verifies the updated state.

## When To Use

- Any code change under `boot/`, `kernel/`, `tools/`, or build scripts.
- Any configuration change under `os_cfg.toml`, `Cargo.toml`, `Makefile`.
- Any API/architecture behavior change that impacts existing docs.
- Any newly created docs or skills files that require nav/index updates.

## Non-Negotiable Rules

- Every meaningful code/config change must include a docs/skills impact decision.
- Any API or externally observable behavior change must update related pages in `docs/api/**` in the same change.
- Any architecture-specific behavior change must update related pages in `docs/api/arch/**` and `docs/implementation/**` in the same change.
- Keep runtime kernel code (`kernel/src/**` except `kernel/src/tests/**`) free of test/demo-only logic, assets, and naming.
- Keep kernel tests under `kernel/src/tests/**` and organize by subsystem folders instead of flat file growth.
- Keep tests/demo functionally complete: cover main paths, key branches, failure paths, and recovery paths with explicit assertions.
- Keep tests/demo logs detailed and step-level: include action, input, expected result, actual result, and failure location.
- Keep tests/demo coverage comprehensive: include invalid input, unexpected input, and boundary-condition cases.
- Every minimum-scope implementation (minimal set / temporary subset) must add or update an item in `docs/progress/tech-debt.md` in the same change.
- Every minimum-scope debt item must include current gap, full target, repayment version, and clear closure signal.
- When repayment is implemented in a later version, update `docs/progress/tech-debt.md` and related docs/api pages in the same change.
- If no docs/skills update is needed, explain exactly why in final report.
- Do not leave `docs/.vitepress/config.ts` stale when adding/renaming docs pages.
- Keep skill metadata (`SKILL.md` frontmatter and `agents/openai.yaml`) consistent.
- Architecture-specific code must live under architecture paths (`boot/<arch>/`, `kernel/src/**/arch/<arch>/`); do not keep arch logic in generic modules without explicit abstraction.

## Workflow

1. Detect change set.
Use `git status --short` and `git diff --name-only` to list modified files.

2. Map change set to required information updates.
Apply `references/update-matrix.md` to decide required docs/skills updates.
Explicitly decide whether the change introduces/extends/repays any minimum-scope implementation debt.

3. Update docs.
Edit relevant pages in `docs/` and update `docs/.vitepress/config.ts` nav/sidebar entries when paths or topics changed.
If minimum-scope behavior is introduced or changed, update `docs/progress/tech-debt.md` in the same change.

4. Update skills.
If workflow, architecture assumptions, or verification commands changed, update:
- `skills/january-os-project-intel/*`
- `skills/january-os-docs-skills-sync/*`

5. Validate.
Run appropriate checks:
- `make build` for code/build related updates.
- `cd docs && pnpm build` for docs updates.
- skill validation script for changed skills.

6. Emit final sync report.
Use `references/report-template.md` and include exact changed file paths.

## Resources

- `references/update-matrix.md`: path-to-docs/skills update rules.
- `references/report-template.md`: required final report structure.
