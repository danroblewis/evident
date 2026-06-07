# stage-0 sizing spike (2026-06-07)

Question sized: how big is "stage-0" — the minimal kernel-runnable
compiler able to compile `compiler2.ev` (the Z3-AST compiler of
`z3-ast-compiler2-plan.md`, written in a subset we define) — under
the stitch architecture:

  (a) a HAND-WRITTEN generic multi-tick capture-driver `.smt2` shell
      supplying the four capabilities the fossil cannot emit
      (conditional effects writer, `last_results` readback, payload
      extraction, state-phase machine);
  (b) FOSSIL-COMPILED node-dispatch claims (match→ite, bare-name
      composition, single-binop pins — all in `fossil-subset.md`);
  (c) a stitcher (`scripts/stitch-stage0.sh`, transition tooling)
      that splices (b)'s emitted claim bodies into (a) under a
      naming discipline.

## The working toy (the evidence)

`tests/seam/stage0_toy/` — built and RUN during this spike:

```
echo "x = 5" | kernel tests/seam/stage0_toy/toy.smt2
  → prints "(= x 5)", exit 0
echo "x + 5" | kernel … → prints "stage0: parse error", exit 5
echo "y = 7" | kernel … → prints "(= y 7)", exit 4
```

The toy reads one line via `ReadLine`, splits it into three fields
(shell-side SMT string theory), classifies each field through THREE
stitched instances of one fossil-compiled classifier
(`lexkind.ev` → cascaded match→ite over a `Result`-encoded char
class), combines the kinds through a fossil-compiled dispatcher
(`parsedispatch.ev` → bare-name composition `ShapeWeights` + four
single-binop pins, shape = ka·100 + kb·10 + kc), gates on
shape = 132 (conditional effects writer in the shell), then builds
the Z3 AST `(= x 5)` via seven libz3 LibCalls
(config/context/int_sort/string_symbol/const/int/eq, one phase
each), reads back `Z3_ast_to_string` through `__cstr.copy`, prints,
and exits 0 iff the render matches. The exit-4/exit-5 runs prove the
fossil-compiled dispatch actually gates and that input data flows
through the spliced claims into the AST — nothing is hardcoded.

Phase trace (EVIDENT_PHASE_TRACE=1) shows the expected
1-effect-per-tick capture chain: ReadLine → 11 LibCalls → Exit(0).
Functionizer: 39 residual asserts, ~1.6 ms z3 per tick.

### Two NEW fossil-subset cliffs found while building it

Both reproduced with one-suspect fixtures; both are additions to
`fossil-subset.md`'s tables (probed against the same artifact, md5
`0e5a9f96b29196f4688efcc3cd1fc3df`):

1. **Multi-arm match MISCOMPILES-LOUD + drains.** A 4-arm match
   emitted `(= lk_kind (ite ((_ is StringResult) lk_cls) 1 _))` —
   the default `_` leaks as a literal underscore symbol, arms 2–3
   vanish, body drains. p46/p49 were 2-arm only. Rule: matches must
   be CASCADED 2-arm (`Variant(_) ⇒ atom`, `_ ⇒ name-of-next-stage`).
   The cascade compiles cleanly (see `lexkind.out.smt2`).
2. **A digit anywhere in an identifier DRAINS-SILENTLY.**
   `pb1 ∈ Int = 9` vanishes with everything after it
   (`tests/seam/stage0_toy/probe_digit_ident.ev` +
   `probe_digit_ident.out.smt2`, committed). Every passing p00–p70
   probe used letter-only locals, so this was latent. Rule:
   fossil-compiled identifiers are `[a-z_]` only.

That two new cliffs surfaced in the first two compiles of a
50-line-total spike is itself a sizing datum (risk 3 below).

## Measured numbers (toy)

| Artifact | Lines | Notes |
|---|---|---|
| `shell.smt2.tpl` (hand-written) | 159 total / 94 non-comment | 19 fixed preamble, 20 captures (2/handle × 10), 14 lexer define-funs, 7 bindings, 3 effects decls, ~31 phase machine (12 phases) |
| `lexkind.ev` | 29 (12 body) | → 10 emitted body lines, spliced 3× by rename |
| `parsedispatch.ev` | 29 (14 body) | → 13 emitted body lines |
| `scripts/stitch-stage0.sh` | 69 (≈30 logic) | grep-extract by registered prefix + sed rename |
| `toy.smt2` (stitched) | 203 | runs as shown above |

- Driver overhead per captured libcall: 2 lines (declare pair +
  capture assert) + ~2.6 lines (linear phase arm) ≈ **4.6 lines**;
  a conditional phase arm ≈ 6 lines.
- Per cascade step in a dispatch claim: 3 source lines → 2 emitted
  lines (declare + ite assert).
- Fossil compile wall time: **43 s and 67 s** measured (one file
  each; instantiation count is free — the stitcher renames, so one
  compile yields N instances).
- Spliced fossil output ≈ 0.9× its `.ev` source lines — the fossil's
  line-count leverage is THIN. Its real value is that per-node
  semantics stay as Evident source (which compiler2 later
  recompiles, keeping the ladder honest) and that match→ite +
  composition inlining are correctness-critical pieces we don't
  hand-write.

## Full stage-0 projection

Assumed stage-0 duty: compile `compiler2.ev` written in a
line-oriented subset WE define (richer than the fossil subset —
multi-arm match w/ payload binding, guarded effects, last_results
reads — since stage-0's parser/emitter is ours). ~12 statement
shapes (claim decl, membership decl, pin-with-expr, bare assert,
comparison, match, enum decl, ctor app, guarded effects, effects
concat, bare-name compose, last_results idiom), ~24 token kinds.
Input via one `ReadFile` (whole source in a String state field,
cursor Int — no FTI needed on the input side); output via
`Z3_solver_assert` accumulation + one `Z3_solver_to_string`;
manifest header via shell-side `str.++`.

Arithmetic from measured bases:

| Component | Basis | Projection |
|---|---|---|
| Shell: fixed preamble | 19 measured | ~25 |
| Shell: captures | 2 lines/handle; ~22 handles (ctx+solver 6, per-shape scratch 8, loop registers 8) | ~45 |
| Shell: tokenizer define-funs | 14 for 3 fixed fields | ~75 (8 token positions × 4 + 24 kind classifiers + escapes) |
| Shell: phase machine | 2.6/linear arm, 6/conditional arm | 12 shapes × ~6 emit phases × ~3 + ~15 conditional control arms × 6 ≈ ~300 |
| Shell: bindings | 1/wire | ~50 |
| **Shell total** | 94 measured | **~500–650 non-comment** |
| Dispatch `.ev` | 3 lines/cascade step | token classifier ~26 steps + shape dispatcher ~14 + code tables ~20 ≈ **~180–220 lines, ~6 files** |
| Stitcher | 69 measured | ~100 (add dedupe + a sentinel-survival verify pass) |
| **stage-0 total** | 286 toy | **≈ 800–1,000 lines** |

Build cost: ~6 fossil compiles ≈ 6 min sequential (parallelizes).
Runtime: toy measured ~1.6 ms z3/tick at 39 asserts; stage-0 at
~10× asserts → ~10–20 ms/tick; compiler2.ev at ~2–4 k flat lines ×
~8–12 ticks/line → 25–50 k ticks ≈ **10–30 min per compile of
compiler2** (needs the existing `EVIDENT_TICK_LIMIT` override above
100 k headroom only if the line estimate doubles).

## The stitcher contract (proven by the toy)

1. Every claim-local name in a fossil-compiled `.ev` begins with a
   per-file prefix, `[a-z_]` only (digits drain — see above), never
   a substring of a shell symbol or another prefix.
2. Template marker: `;; @splice <emit-file> <old_>=<new_> …` at line
   start. Identity renames (`pd_=pd_`) register prefixes without
   renaming; non-identity renames instantiate one compiled claim N
   times (rename IS the parameterization the parameterless subset
   lacks).
3. Extraction: keep `(declare-fun <prefix>…)` lines and `(assert …)`
   lines mentioning a registered prefix; drop everything else (the
   fossil preamble — manifest, datatypes, is_first_tick /
   last_results / effects — carries no prefix). The fossil emits one
   declare/assert per line, which makes this a grep.
4. Each claim keeps a `<prefix>_sentinel ∈ Int = 42` last line; its
   survival in the emit is the junk-drain canary (and it splices
   harmlessly).
5. The shell wires claims with 1-line equality asserts
   (`(= lxa_cls (classify fld1))`, `(= pd_ka lxa_kind)`); claim
   variables are tick-local (not in `state-fields`), so they re-solve
   every tick against current state — no carry plumbing needed.

## Top 3 risks

1. **Loops in the phase machine are unproven.** The toy is linear
   (12 phases, one pass). Stage-0 needs `phase` to branch backward
   (per-statement loop, per-expr-node loop) driven by a cursor.
   Nothing in the kernel contract forbids it — `phase` is ordinary
   state we compute — but it is the one architectural element the
   toy did not exercise. Probe next (a 3-phase loop that sums the
   digits of a line would settle it for ~30 min of work).
2. **Z3 string theory per tick at scale.** The shell's tokenizer is
   pure SMT string ops re-evaluated every tick. In the toy all
   strings are ground (everything pinned) and cost 1.6 ms/tick, but
   `str.indexof`/`str.substr` chains over a multi-KB source String
   on every one of ~40 k ticks is unmeasured; a nonlinear blowup
   here is the most likely schedule killer. Mitigation if hit: hold
   the source in FTI memory (`__mem.read_long` page reads) instead
   of a String state field, at +1 tick per read.
3. **Unmapped fossil cliffs keep surfacing.** Two new ones (multi-arm
   match, digit identifiers) appeared in this spike's first two
   compiles. Worse, the p57-class (MISCOMPILES-SILENT inside an
   accepted body) cannot be caught by the sentinel. Budget one
   compile-inspect cycle (~1 min) per dispatch-claim edit, and
   script a golden-emit diff per claim into the stitcher's verify
   pass before trusting any new construct.

## GO / NO-GO

**GO — qualified.** The architecture is real: every seam (fossil
compile → prefix extraction → rename instantiation → shell binding →
conditional effects → capture chain → libz3 AST build → readback)
ran end-to-end today with correct gating on real input. Projected
size is ~800–1,000 lines (~3.5× the toy), of which the hand-written
shell is ~60% — significant but bounded, and the per-component
arithmetic above comes from measured pieces, not vibes.

Qualifications, in order:
- Settle risk 1 (loops) with a half-day probe BEFORE committing to
  the full shell; a NO there converts this to NO-GO since linear
  phase chains cannot express a parser.
- Accept that the shell is the largest hand-written `.smt2` artifact
  in the repo and is disposable scaffolding: it exists only until
  compiler2 self-compiles, then dies like the bootstrap did.
- The alternative — wave-5a/5b/5c first, then rebuild the fossil
  properly — remains the roadmap's path for the kernel shrink; this
  stage-0 neither replaces nor blocks it. Stage-0 is the cheaper
  (weeks-scale, ~1 k lines) route to a compiler2 that can already
  emit through Z3 ASTs, vs months for the full wave-5 ladder.
