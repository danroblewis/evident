# Program Code-Size Reduction Plan

## Premise

Conciseness and expressivity are core Evident values. The language has
several features specifically designed to reduce line count — multi-name
memberships, first-line type params, names-match composition, passthroughs,
guarded claim invocation, `coindexed`/`edges`, type-use pins — but not
every program in `programs/` uses them consistently.

This plan identifies and applies reductions across the repo, with a bias
toward:

- **Replacing equality chains** (`x = a; y = b; z = c`) with structural
  patterns that the solver can satisfy in any of several ways.
- **Lifting repeated patterns** into shared claims or types.
- **Allowing degrees of freedom** when they make the model smaller and
  the solver doesn't need full determinism.
- **Letting Z3 do the work** — fewer literal constants, more derived
  relationships.

## Multi-phase structure

### Phase 1: Verification (sequential)

- Run `evident test programs/` to establish baseline (DONE: 34 pass, 2
  pre-existing parse failures in adventure*/ — unrelated to this work).
- Note which programs are still active vs. orphaned.

### Phase 2: Parallel research

Dispatch independent subagents in parallel — each focused on one
reduction lens. Each produces a short report with concrete file:line
findings, no edits.

- **Agent A: structural survey.** Enumerate every `programs/**/*.ev`
  file. For each, report line count, claim count, equality-constraint
  count, use of multi-name decls, use of passthroughs / claim calls.
  Output: a table that lets us spot the size outliers and the files
  least leveraging language features.

- **Agent B: equality-density audit.** Identify files where equality
  constraints dominate (`var = literal` pinning everything). For each,
  flag specific line ranges that could become invariants the solver
  satisfies more flexibly. Concrete example: `state.platforms[0].pos =
  IVec2(300, 440) ∧ state.platforms[0].size = IVec2(120, 20)` repeated
  five times — could be a `Seq` literal or a generator.

- **Agent C: composition opportunities.** Look for repeated structure
  across files in the same directory. Where two programs duplicate
  ~30+ lines, propose extracting into a shared claim or a passthrough.
  Examples likely include the SDL/audio plugin setup boilerplate, the
  init-claim pattern, the per-frame render glue.

- **Agent D: language-feature-gap audit.** Cross-reference programs
  against `CLAUDE.md`'s "Style: keep main compact" section and
  `docs/design/program-structure.md`. Identify programs that violate
  the canonical patterns (long mains, indexed loops where `coindexed`
  fits, scalar decompositions where vector types fit).

### Phase 3: Synthesis

Combine the four reports into a prioritized punch list:

- Per-file: estimated line savings + reduction technique.
- Per-pattern: the abstraction worth introducing, and which files would
  consume it.

### Phase 4: Apply (in priority order)

For each item on the punch list:

1. Make the change.
2. Re-run `evident test programs/<dir>/` for the affected dir.
3. If the program is interactive (SDL), also smoke-run for 3 seconds
   to confirm no regression.
4. Commit per logical reduction (one technique per commit) so the
   diffs read as "this is how the technique applies."

### Phase 5: Codify

For each reduction technique that landed in 2+ files, add a section
to CLAUDE.md or `docs/design/program-structure.md` so future
programs adopt it from the start. The goal is the documented
patterns reflect the actual codebase, not aspirational style.

## Constraints

- **Don't break tests.** Every change runs through `evident test
  <affected-dir>` before commit.
- **Don't change semantics.** Reductions preserve behavior; if a
  reduction produces a less-determined program, the solver's
  satisfying assignment must still produce the same observable
  effects (same renders, same outputs, same SAT/UNSAT for tests).
- **Acceptable degrees of freedom.** A program where multiple
  satisfying assignments exist is fine if all of them produce the
  same observable behavior. E.g., `0 ≤ x ≤ 100` instead of `x = 50`
  is acceptable when downstream code doesn't depend on the specific
  value.
- **Don't introduce abstractions for hypothetical reuse.** A pattern
  needs ≥ 2 actual consumers to justify a shared claim. One-off
  patterns stay inline.
