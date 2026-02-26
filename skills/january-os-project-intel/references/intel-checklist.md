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
- Action Plan

## Minimum Quality Bar

- Every claim must be traceable to file content or command output.
- Include exact file paths in conclusions.
- Explicitly list unknowns and next command/file needed to close each unknown.
