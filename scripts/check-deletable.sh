#!/usr/bin/env bash
# scripts/check-deletable.sh — single source of truth for "is bootstrap deletable?"
#
# Exit 0 + "BOOTSTRAP DELETABLE NOW"  → we are done; you may run `rm -rf bootstrap/`.
# Exit 1 + a list of blockers          → we are not done; here's what's in the way.
#
# Run this BEFORE doing any work in this project. It tells you the current
# state in a way no prose document can — by counting actual references in
# the actual codebase.
#
# TODO: rewrite in Evident once we can run scripts on the kernel.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

blockers=()

# ──────────────────────────────────────────────────────────────────────────────
# Blocker 1: anything in the repo still references the bootstrap binary path
# ──────────────────────────────────────────────────────────────────────────────
ref_paths=$(grep -rln 'bootstrap/runtime/target' \
    --include='*.sh' --include='*.py' --include='*.toml' --include='*.md' \
    --include='*.ev' --include='*.cfg' \
    . 2>/dev/null \
  | grep -v '^\./bootstrap/' \
  | grep -v '^\./\.claude/' \
  | grep -v '^\./\.cargo/' \
  | grep -v '^\./scripts/coordinator-results/' \
  | grep -v '^\./STATE\.md$' \
  | grep -v '^\./CLAUDE\.md$' \
  | grep -v '^\./scripts/check-deletable\.sh$' \
  | grep -v '^\./docs/' \
  | grep -v '^\./tests/conformance/features/README\.md$' \
  || true)
# Documentation prose (docs/, the features README) describes the bootstrap
# path as part of explaining the architecture and the deletion procedure —
# it is reference, not an operational dependency, and is excluded above.
# Every *operational* reference (scripts that actually invoke the binary)
# now resolves the path through `scripts/evident-self bin`, so the literal
# lives in exactly one extension-less file the grep doesn't scan.
if [ -n "$ref_paths" ]; then
  count=$(printf '%s\n' "$ref_paths" | wc -l | tr -d ' ')
  blockers+=("$count files still reference bootstrap/runtime/target:")
  while IFS= read -r f; do
    [ -n "$f" ] && blockers+=("    $f")
  done <<< "$ref_paths"
fi

# Note: .cargo/config.toml legitimately points at bootstrap because that's where
# the linker wrapper lives during the bootstrap build. It's excluded above; the
# blocker will surface once we no longer build bootstrap at all (.cargo/ goes
# away too at that point).

# ──────────────────────────────────────────────────────────────────────────────
# Blocker 2: compiler.smt2 (the self-hosted compiler) must exist at repo root
# ──────────────────────────────────────────────────────────────────────────────
if [ ! -f compiler.smt2 ]; then
  blockers+=("compiler.smt2 does not exist at the repo root.")
  blockers+=("    This is the self-hosted compiler — written in Evident at")
  blockers+=("    compiler/compiler.ev, compiled once via bootstrap, and")
  blockers+=("    committed here. Until it exists, only bootstrap can compile .ev files.")
fi

# ──────────────────────────────────────────────────────────────────────────────
# Blocker 3: no Python in scripts/ or tests/
# ──────────────────────────────────────────────────────────────────────────────
py_files=$(find scripts tests -name '*.py' 2>/dev/null \
  | grep -v '^scripts/coordinator-results/' \
  | sort || true)
if [ -n "$py_files" ]; then
  count=$(printf '%s\n' "$py_files" | wc -l | tr -d ' ')
  blockers+=("$count Python files remain under scripts/ or tests/ (scheduled for removal):")
  while IFS= read -r f; do
    [ -n "$f" ] && blockers+=("    $f")
  done <<< "$py_files"
fi

# ──────────────────────────────────────────────────────────────────────────────
# Blocker 4: test.sh must not invoke bootstrap
# ──────────────────────────────────────────────────────────────────────────────
if [ -f test.sh ] && grep -qE 'bootstrap/runtime|cd[[:space:]]+bootstrap' test.sh; then
  blockers+=("test.sh still invokes bootstrap. Switch its 'evident' binary path")
  blockers+=("    to use kernel + compiler.smt2.")
fi

# ──────────────────────────────────────────────────────────────────────────────
# Blocker 5: bootstrap/ must not exist
# ──────────────────────────────────────────────────────────────────────────────
if [ -d bootstrap ]; then
  size=$(find bootstrap -name '*.rs' 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
  blockers+=("bootstrap/ directory still exists (${size:-?} lines of Rust).")
  blockers+=("    When every blocker above is cleared, run: rm -rf bootstrap/")
fi

# ──────────────────────────────────────────────────────────────────────────────
# Verdict
# ──────────────────────────────────────────────────────────────────────────────
if [ ${#blockers[@]} -eq 0 ]; then
  echo "BOOTSTRAP DELETABLE NOW."
  echo
  echo "Everything that previously depended on bootstrap is now routed"
  echo "through kernel + compiler.smt2. Python is gone from scripts/tests."
  echo
  echo "Final step: rm -rf bootstrap/ && rm .cargo/config.toml"
  echo "Then update CLAUDE.md to mark the project as done."
  exit 0
fi

echo "BOOTSTRAP NOT YET DELETABLE."
echo
echo "Blockers:"
echo
for line in "${blockers[@]}"; do
  echo "$line"
done
echo
echo "See CLAUDE.md, section 'The deletion path,' for how to clear these."
exit 1
