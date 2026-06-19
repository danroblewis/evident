---
name: audit-cruft
description: Find and remove incidental complexity from the Evident runtime, judged against docs/design/core.md. Use when the user wants to minimize toward the core, cut cruft (including useful cruft), reduce the runtime, or audit whether a subsystem belongs. NOT a line-count chase — the cut decision is "core vs incidental," and every candidate must be justified against core.md before removal.
---

# audit-cruft — reduce to the core, by judgment, not reflex

The goal is a **solid, minimal, navigable core**, not a smaller line count. Lines
are a proxy. Read [`docs/design/core.md`](../../docs/design/core.md) FIRST — it is
the yardstick for every decision below. Do not decide on caller-count or
line-count alone.

## The loop (repeat until nothing incidental remains)

1. **Anchor.** Read `docs/design/core.md`. Hold its core pipeline + cut rule in
   mind for every candidate.

2. **Discover** candidates with Semfora + judgment:
   - `get_overview` / `get_module` to see subsystems; **symbol-hash collisions in
     the symbol list = clones** (Semfora's strongest signal).
   - `dead_code_audit` / `find_duplicates` are HINTS ONLY — they miss exact clones
     and flag every `#[test]`/`pub` fn. Never act on them directly.
   - Coverage over real `effect-run` demos (instrument build + `LLVM_PROFILE_FILE`,
     report via rustup `llvm-tools`) shows what real programs never execute.
   - Your own semantic read: speculative infra, half-features, alternate paths,
     conveniences — the "not core" list in core.md.

3. **Measure containment** for each candidate:
   - Semfora `get_callers` / `trace` for blast radius — then **grep-verify**.
     Semfora's caller graph UNDERCOUNTS cross-module `rt.method()` calls and ALL
     test-file calls. Never trust its count alone.
   - `get_source` / read the actual code. **"0 callers" is a hypothesis, never a
     verdict** — it can mean dead, a `#[test]` (tests have no callers), or pub-API.

4. **Judge against core.md — REQUIRED, with a one-line reason per candidate:**
   - **CORE** → protect, do not touch.
   - **INCIDENTAL** → cut. ("useful" is not a defense; half-features are worse
     than absent ones; when in doubt, cut and defer.)
   - **ROADMAP** → leave for now; it returns as a bounded subsystem, not a tendril.
   - **LIBRARY** → refactor out of the core rather than delete.
   Write the verdict + reason before removing anything. If you can't justify
   "incidental" against core.md, leave it and surface it to the user.

5. **Remove** each incidental item as its own commit. `cargo build` + `./test.sh`.
   If `./test.sh` fails → it was load-bearing → revert and re-judge. SDL demos:
   verify with `./test.sh --examples-only` + Read the PNGs.

6. **Go further (the completeness critic).** After a pass, ask explicitly: *"What
   is still in the tree that is NOT in core.md's core?"* If the honest answer is
   "only judgment calls," surface those to the user. Otherwise, loop.

## Orchestration discipline (worktree agents) — traps that bit us

- One worktree per agent. Anchor cwd to the worktree for **every** command and use
  absolute worktree paths — the first `cd` to the repo can resolve to the shared
  checkout (cwd-drift; it has corrupted `main` and bitten three runs).
- **Index the worktree for Semfora.** Semfora is per-path indexed and defaults to
  `main`, so without this its tools analyze main's code, not your worktree's. After
  `git merge main`, run `semfora-engine index generate .` inside the worktree;
  re-index after significant edits. (grep-verify still runs on your worktree files,
  so it catches staleness regardless — but index the worktree so discovery is right.)
- **Verify the merge commit exists before ANY cleanup.** Never chain
  merge → `worktree remove` → `branch -D` in one shot.
- **Never pipe a merge through `tail`/`head`** — it masks the exit code so a
  failed merge looks successful.
- After deleting a branch, its commits survive as dangling objects — recover by
  merging the commit hash directly.
