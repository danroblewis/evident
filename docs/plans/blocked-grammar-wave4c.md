# Blocked: wave-4c item 3 — the deletion-readiness smoke test (updated chain)

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
still does not exit 0. Wave 4c **closed wave-4b blocker 0** (the dominant
Seq/effects encoding mismatch — proven end-to-end, see
`docs/plans/grammar-wave4c.md` item 3). This note records the chain that
remains, re-prioritised, with the NEW blocker that the encoding fix
surfaced now at the top.

Empirical method: drive the REAL self-hosted compiler (`kernel +
compiler.smt2`) on progressively richer inputs and read the FIRST kernel
error each time. As blocker 0 cleared, the error advanced from
`unknown constant last_results__len` (was unreachable behind the seq-theory
failure) to the prelude gap below.

## Blocker 0 — RESOLVED ✓ (Seq/effects encoding)

The self-hosted compiler emits `(Array Int T) + <name>__len` with per-index
`(select …)` asserts and a derived `max-effects`. The kernel runs the
emitted effects channel. Verified by a full compile→run round trip to
exit 0 on a minimal `enum Effect = Exit(Int) / claim hello / effects ∈
Seq(Effect) = ⟨Exit(0)⟩` (with the prelude of blocker A prepended). See
`docs/plans/grammar-wave4c.md`.

## Blocker A (NEW, now DOMINANT) — the emit prelude is not produced

The kernel ALWAYS pins `last_results__len` and decodes `last_results` as an
`(Array Int Result)` (`kernel/src/tick.rs`). Bootstrap's `emit.rs`
hand-writes this prelude — `result_and_last_results_decls()` emits the
`Result` datatype + `(declare-fun last_results () (Array Int Result))`, and
the body adds `(declare-fun last_results__len () Int)` +
`(assert (>= last_results__len 0))`. The self-hosted EMIT phase produces
NONE of it, so a self-hosted program dies at:

```
$ kernel <self-hosted-output.smt2>
Error: (error "line N: unknown constant last_results__len")
```

Confirmed: prepending exactly those three decls (`Result`, `last_results`,
`last_results__len`) to the self-hosted output makes the minimal program
run to exit 0.

**Reference (`bootstrap emit hello`, the target):**
```
(declare-datatypes ((Result 0)) (((NoResult) (IntResult (IntResult__f0 Int))
  (StringResult (StringResult__f0 String)) (RealResult (RealResult__f0 Real))
  (EofResult) (ErrorResult (ErrorResult__f0 String)))))
(declare-fun last_results () (Array Int Result))
…
(declare-fun last_results__len () Int)
(assert (>= last_results__len 0))
```

**Unblock:** add a fixed prelude string to `compiler.ev`'s EMIT assembly
(between the manifest header and the datatype/declare blocks), mirroring
`emit.rs` `result_and_last_results_decls` + the `last_results__len` decl.
This is a mechanical, self-contained string-constant addition — the highest
-value next step, and it unblocks EVERY self-hosted program, not just
`test_hello`. (It was previously invisible: the seq-theory output failed in
the kernel before reaching the `last_results` pin.)

## Blocker 1 (unchanged) — comment stripping in the consolidated lexer

Flattened `test_hello` is ~70% `--` comment lines; the lexer has no comment
mode and emits no `Newline`, so a `--` lexes as two `Minus` tokens + ident
words and mis-routes dispatch. Add a comment mode to the char-level lexer
FSM (on `-` with lookahead `-`, consume to `\n`, emit nothing).

## Blocker 2 (unchanged) — enum variants with nested compound field types

`Effect`'s `LibCall(String, String, Seq(LibArg))` third field type is
`Seq(LibArg)`; the e6/e7 enum field-type states read `Seq` as the type and
stall on `(`. Extend the field-type reader to accumulate a compound type
string (reuse `parser.ev::TypeTokenText` + `BracketDelta`).

NOTE: the kernel ALSO needs the nested `Seq(LibArg)` payload encoded as
bootstrap's `__SeqOf_LibArg` cons datatype, NOT `(seq.unit …)`. Wave 4c's
Array+len applies to the OUTER state field only; the in-ctor `Seq` payload
(`⟨ArgStr("hi")⟩` inside `LibCall`) still renders to `(seq.unit (ArgStr
"hi"))` via `RenderExprL*`. So even with the enum parsed, a `LibCall`
effect is not yet kernel-decodable. This is a second facet of blocker 2.

## Blocker 3 (unchanged) — parametrized claims + claim selection

The inlined `Build*` sugar claims carry first-line params `(s ∈ String, eff
∈ Effect)` and bodies; the claim sub-machine assumes `<kw> Ident` then
memberships (a `(` after the name is unhandled), and the driver emits EVERY
claim it walks while the reference emits ONLY the target claim. Unblock:
(a) parse-and-skip first-line params; (b) a claim-selection mechanism
(emit the last top-level claim, or a registry of referenced claims).

## Blocker 5 (unchanged, now measurable) — per-tick solve cost

The self-hosted compiler now COMPLETES a minimal compile, but at ~40 s for
a ~40-char input (`compiler.smt2` is 200 341 lines; `[functionizer] not
functionized … 37 121 residual`). The driver re-solves its whole body every
char-level lex tick. A multi-KB flattened `test_hello` is intractable until
the renderer becomes a work-stack walker (constraints don't all fire every
tick) or the functionizer extracts the lex/parse steps. The `SeqArrayBlock`
+ triple-`RenderExprToks` composition grew the driver ~6.5× over wave 4b,
making this MORE acute — a real cost of choosing the depth-unrolled renderer
over the work-stack walk.

## Summary (re-prioritised)

(0) Seq/effects Array+len encoding — **RESOLVED this wave.**
(A) **NEW dominant:** emit the `Result`/`last_results`/`last_results__len`
prelude (mechanical string constant). Then, in order: (1) comment
stripping, (2) nested enum field types + the `__SeqOf_T` payload encoding,
(3) parametrized-claim skip + claim selection, (5) per-tick solve cost
(work-stack walker / functionizer). Blocker A is the recommended next
session — smallest, unblocks all self-hosted programs.
