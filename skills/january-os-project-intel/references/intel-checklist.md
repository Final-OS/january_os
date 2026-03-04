# Intel Checklist

## Mandatory Commands

Run from repository root unless task is clearly docs-only:

```bash
git status --short
git branch --show-current
```

For build/runtime context:

```bash
make config
make build
```

For docs-only scope:

```bash
cd docs && pnpm build
```

## Mandatory Files To Inspect

- `AGENTS.md`
- `Makefile`
- `os_cfg.toml`
- `docs/.vitepress/config.ts`
- `skills/january-os-project-intel/SKILL.md`
- `skills/january-os-docs-skills-sync/SKILL.md`

## Intel Report Sections

- Repository State
- Module Map
- Change Impact
- Docs+Skills Coverage
- Minimum-Set Debt Check
- Action Plan

## Minimum Quality Bar

- Every claim must be traceable to file content or command output.
- Include exact file paths in conclusions.
- Explicitly list unknowns and next command/file needed to close each unknown.
- Explicitly check whether API/behavior changes are synchronized to `docs/api/**`.
- Explicitly check whether architecture-specific code is located under `boot/<arch>/` or `kernel/src/**/arch/<arch>/`.
- Explicitly check whether runtime kernel code contains test/demo-only logic, assets, or naming.
- Explicitly check whether `kernel/src/tests/**` is organized by subsystem directories (avoid flat growth).
- Explicitly check whether tests/demo scenarios are functionally complete (main path, key branches, fail/recovery paths) with explicit assertions.
- Explicitly check whether tests/demo logs are step-level and include action/input/expected/actual plus failure location.
- Explicitly check whether tests/demo include invalid input, unexpected input, and boundary-condition cases.
- Explicitly check whether every minimum-scope implementation is tracked in `docs/progress/tech-debt.md` with full-target repayment version and closure criteria.
