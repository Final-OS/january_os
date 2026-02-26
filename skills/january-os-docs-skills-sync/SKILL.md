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
- If no docs/skills update is needed, explain exactly why in final report.
- Do not leave `docs/.vitepress/config.ts` stale when adding/renaming docs pages.
- Keep skill metadata (`SKILL.md` frontmatter and `agents/openai.yaml`) consistent.

## Workflow

1. Detect change set.
Use `git status --short` and `git diff --name-only` to list modified files.

2. Map change set to required information updates.
Apply `references/update-matrix.md` to decide required docs/skills updates.

3. Update docs.
Edit relevant pages in `docs/` and update `docs/.vitepress/config.ts` nav/sidebar entries when paths or topics changed.

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
