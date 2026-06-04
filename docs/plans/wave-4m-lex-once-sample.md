# Wave 4m — lex-once multi-claim `sample` mode

## Goal

Fold **Wall 1** of `docs/plans/blocked-sample-and-eq-fix.md`: the
self-hosted `sample` verb re-lexed the whole file once *per claim*
(`kernel + compiler.smt2` ≈ 90 s × ~190 claims ⇒ hours per `--lang`
pass). Amortise the lex+parse over all claims in ONE kernel run.

## Design choice: Option B (no kernel change)

Two shapes were on the table (task spec + `docs/plans/blocked-bootstrap-cutover.md`
§"The unblock", Option 1):

- **A** — a kernel built-in `check-sat this SMT-LIB fragment → bool`.
  Requires a Rust change to the frozen-on-completion `kernel/`.
- **B** — a SAMPLE driver that emits, in one kernel run, every claim's
  constraints wrapped in `(push) … (check-sat) (pop)`, then a single
  standalone `z3 -in` decides them all.

**Chose B.** No kernel change, reuses the existing `z3` (`/opt/anaconda3/bin/z3`)
the wave-4j wrapper already used, smallest surface area.

## What landed

### `compiler/sample.ev` (new) → `sample.smt2`

A copy of `compiler/compiler.ev` (same lex → reverse → parse-dispatch →
claim/enum/skip sub-machines) with three changes:

1. **`name_selected = true`** — no target-claim selection; *every* item
   is sampled. Bare-head claims → `enter_claim` (translated body),
   parametrized claims/types → `enter_skip` (empty body).
2. **`sacc` accumulator** — a program-global string that appends one
   block per claim, in source order:
   - on `claim_done`: `;; claim: <name>\n(push)\n<ift_decl ++ _out>\n(check-sat)\n(pop)\n`
   - on `skip_stop`:  `;; claim: <name>\n(push)\n(check-sat)\n(pop)\n`
3. **EMIT** — `prelude ++ datatypes ++ _sacc` (shared Result/last_results
   decls + enum datatypes BEFORE the first push, so `(pop)` never
   discards them — the z3 push/pop gotcha), `puts` + `Exit(0)`. No
   manifest header (output is fed to `z3 -in`, not the kernel; z3 ignores
   `;;` lines so the `;; claim:` markers pass through for the wrapper).

`enum` items are NOT sampled (matching bootstrap's `schema_names`, which
excludes enums); their datatypes go to the shared `_eacc` block.

### `scripts/build-sample-smt2.sh` (new)

Mirror of `build-compiler-smt2.sh` for `compiler/sample.ev → sample.smt2`.
One-time bootstrap handoff.

### `scripts/sample-via-smt2.sh` (rewritten)

Per-claim loop DROPPED. Now: flatten once → one `kernel sample.smt2` run
→ pipe to `z3 -in` → zip the `;; claim:` markers (emit order) against
z3's sat/unsat lines (same order) → JSON.

## Fidelity notes / known gaps

- **Param claims/types emit an EMPTY body ⇒ trivially `sat` (true).**
  This matches bootstrap for every param claim/type in the lang corpus
  (all `true`: `Color`, `IVec2`, `Outer`, `use_color`, `BuildPrintln`,
  …). It is a fidelity gap only for a param claim whose body is
  self-contradictory — which the corpus never exercises. Closing it
  needs the compiler to translate param-claim bodies (a later wave).
- **Wall 2 (claim-body shape gaps)** is orthogonal and handled by waves
  4k/4l. Where a lang file uses a shape the membership walk still drops,
  the self-hosted verdict can differ from bootstrap; those are shape
  gaps, not amortisation bugs.

## Results

<!-- FILLED IN AFTER MEASUREMENT -->
