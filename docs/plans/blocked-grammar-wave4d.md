# Blocked: wave-4d — the deletion-readiness smoke test (chain after the prelude)

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
still does not exit 0. Wave 4d **closed wave-4c Blocker A** (the emit
prelude — `Result` / `last_results` / `last_results__len`; proven by a
full self-hosted round trip to exit 0 on a minimal program — see
`docs/plans/grammar-wave4d.md`). This note records the chain that
remains, re-prioritised, with Blocker A struck.

Empirical method unchanged: drive the REAL self-hosted compiler
(`kernel + compiler.smt2`) on progressively richer inputs and read the
FIRST failure each time.

## Blocker A — RESOLVED ✓ (emit prelude)

`compiler/compiler.ev` now emits a fixed `prelude` string (the `Result`
datatype + `(declare-fun last_results () (Array Int Result))` +
`(declare-fun last_results__len () Int)` + `(assert (>= last_results__len
0))`) between the manifest and the body, byte-identical to bootstrap's
`emit.rs:147` `result_and_last_results_decls()` + the injected-membership
`last_results__len` decl/assert. A minimal program now compiles via
`kernel + compiler.smt2` AND the emitted `.smt2` runs to exit 0. Before
this wave it died at `unknown constant last_results__len`.

## Blocker 5 (now the gating cost for ANY rich input) — per-tick solve cost

The full `test_hello` flattens to 4137 chars / 106 lines. The driver
re-solves its whole 200 348-line body every char-level lex tick at
~40–60 s/tick (`[functionizer] not functionized … 37 123 residual`).
4137 ticks × ~tens-of-seconds = intractable; the self-hosted compile of
the full input does not complete inside any practical timeout. **This is
now the dominant blocker for moving past minimal inputs** — blockers 1/2/3
below cannot even be reached on a real corpus file until the renderer
stops re-solving everything each tick. Unblock: a work-stack walker (fire
only the constraints the current step needs) OR functionizer extraction
of the lex/parse steps so they JIT instead of going to Z3.

## Blocker 1 (unchanged) — comment stripping in the consolidated lexer

Flattened `test_hello` is ~70 % `--` comment lines; the lexer has no
comment mode and emits no `Newline`, so a `--` lexes as two `Minus`
tokens + ident words and mis-routes dispatch. Add a comment mode to the
char-level lexer FSM (on `-` with lookahead `-`, consume to `\n`, emit
nothing).

## Blocker 2 (unchanged) — enum variants with nested compound field types

`Effect`'s `LibCall(String, String, Seq(LibArg))` third field type is
`Seq(LibArg)`; the e6/e7 enum field-type states read `Seq` as the type
and stall on `(`. Extend the field-type reader to accumulate a compound
type string (reuse `parser.ev::TypeTokenText` + `BracketDelta`).

NOTE: the kernel ALSO needs the nested `Seq(LibArg)` payload encoded as
bootstrap's `__SeqOf_LibArg` cons datatype, NOT `(seq.unit …)`. Wave 4c's
Array+len applies to the OUTER state field only; the in-ctor `Seq` payload
(`⟨ArgStr("hi")⟩` inside `LibCall`) still renders to `(seq.unit (ArgStr
"hi"))` via `RenderExprL*`. So even with the enum parsed, a `LibCall`
effect is not yet kernel-decodable. Second facet of blocker 2.

## Blocker 3 (unchanged) — parametrized claims + claim selection

The inlined `Build*` sugar claims carry first-line params `(s ∈ String,
eff ∈ Effect)` and bodies; the claim sub-machine assumes `<kw> Ident`
then memberships (a `(` after the name is unhandled), and the driver
emits EVERY claim it walks while the reference emits ONLY the target
claim. Unblock: (a) parse-and-skip first-line params; (b) a
claim-selection mechanism (emit the last top-level claim, or a registry
of referenced claims).

## Summary (re-prioritised)

(A) emit prelude — **RESOLVED this wave.**
(5) **NEW dominant for rich inputs:** per-tick solve cost — the renderer
re-solves everything every char tick, so no multi-KB corpus file
completes. Must become a work-stack walker (or be functionizer-extracted)
before blockers 1/2/3 are reachable on a real file. Then, on a
comment-free / single-claim / simple-enum basis the chain is: (1) comment
stripping, (2) nested enum field types + the `__SeqOf_T` payload encoding,
(3) parametrized-claim skip + claim selection. Recommended next session:
attack blocker 5 (work-stack walker) — it gates everything else, and the
minimal round trip already proves the back-end (encoding + prelude) is
correct, so the remaining work is front-end traversal cost, not output
shape.
