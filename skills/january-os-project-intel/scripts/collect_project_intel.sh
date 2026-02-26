#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-$(pwd)}"
cd "$ROOT_DIR"

echo "=== january_os project intel ==="
echo "timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "root: $(pwd)"
echo

echo "== git state =="
git branch --show-current || true
git rev-parse --short HEAD || true
git status --short || true
echo

echo "== key files =="
for f in AGENTS.md Makefile os_cfg.toml Cargo.toml docs/.vitepress/config.ts; do
  if [ -f "$f" ]; then
    printf "present: %s\n" "$f"
  else
    printf "missing: %s\n" "$f"
  fi
done
echo

echo "== top-level dirs =="
find . -maxdepth 2 -type d \
  | sed 's|^\./||' \
  | sort
echo

echo "== docs pages =="
find docs -type f -name '*.md' | sort
echo

echo "== skills pages =="
find skills -type f | sort
echo

echo "== recent changes (name-status) =="
git diff --name-status HEAD || true
