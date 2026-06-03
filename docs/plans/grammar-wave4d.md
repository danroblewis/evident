# compiler.ev grammar — wave 4d (emit prelude: Result + last_results)

Status: **Item 1 LANDED and proven end-to-end.** The self-hosted
compiler now emits the kernel-required `Result` / `last_results` /
`last_results__len` prelude that bootstrap hand-writes. The wave-4c
DOMINANT blocker (Blocker A) is **RESOLVED**: a minimal program now
compiles via `kernel + compiler.smt2` AND the emitted `.smt2` runs on
the kernel to exit 0 — the full self-hosted round trip.

The full `test_hello` smoke test is still NOT green, but for reasons
unchanged from wave 4c: blockers 1/2/3 (comment stripping, nested enum
field types, parametrized-claim skip) compounded by blocker 5 (per-tick
solve cost makes a 4137-char flattened input intractable). Blocker A is
no longer on that chain.

Cites: `docs/plans/blocked-grammar-wave4c.md` (Blocker A, the precise
spec), `bootstrap/runtime/src/emit.rs`
(`result_and_last_results_decls` + the `last_results__len` decl/assert
the injected `last_results ∈ Seq(Result)` membership produces),
`compiler/compiler.ev` (the EMIT assembly where the prelude lands).
Extends `docs/plans/grammar-wave4c.md`.

## Item 1 — the emit prelude ✓

`bootstrap/runtime/src/emit.rs:147` `result_and_last_results_decls()`
hand-writes the `Result` datatype + `(declare-fun last_results ()
(Array Int Result))`. The `last_results__len` decl +
`(assert (>= last_results__len 0))` come from the injected
`last_results ∈ Seq(Result)` membership (`emit.rs:71`). The kernel
ALWAYS pins `last_results__len` and decodes `last_results` as an
`(Array Int Result)` (`kernel/src/tick.rs`), so every kernel-runnable
program needs all four lines. The self-hosted EMIT phase produced NONE
of them — that was Blocker A.

`compiler/compiler.ev` now emits a fixed `prelude` string constant in
the EMIT assembly, between the manifest header and the enum-datatype /
body declares (mirroring bootstrap's
`{manifest}\n{result_and_last_results}{body_smt}…` order):

```
(declare-datatypes ((Result 0)) (((NoResult) (IntResult (IntResult__f0 Int)) (StringResult (StringResult__f0 String)) (RealResult (RealResult__f0 Real)) (EofResult) (ErrorResult (ErrorResult__f0 String)))))
(declare-fun last_results () (Array Int Result))
(declare-fun last_results__len () Int)
(assert (>= last_results__len 0))
```

Byte-identical to bootstrap's output (the `prelude` constant in
`compiler/compiler.ev`, just after `datatypes`).

## Verification — the round trip

Input (minimal, comment-free, single-claim, simple-enum — isolates
Blocker A from the unrelated blockers 1/2/3):

```
enum Effect = Exit(Int)
claim hello
    effects ∈ Seq(Effect) = ⟨Exit(0)⟩
```

Driven through the REAL self-hosted compiler (`kernel + compiler.smt2`,
built by `scripts/build-compiler-smt2.sh`) it now emits:

```
;; manifest: state-fields =
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 1
(declare-datatypes ((Result 0)) (((NoResult) (IntResult (IntResult__f0 Int)) (StringResult (StringResult__f0 String)) (RealResult (RealResult__f0 Real)) (EofResult) (ErrorResult (ErrorResult__f0 String)))))
(declare-fun last_results () (Array Int Result))
(declare-fun last_results__len () Int)
(assert (>= last_results__len 0))
(declare-datatypes ((Effect 0)) (((Exit (Exit__f0 Int)))))
(declare-fun is_first_tick () Bool)
(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (= effects__len 1))
(assert (= (select effects 0) (Exit 0)))
```

Running THAT emitted `.smt2` on the kernel:

```
$ kernel <emitted>.smt2 ; echo $?
0
```

Before this wave the same program died at
`(error "… unknown constant last_results__len")`. The prelude closes
that gap. This is the complete `.ev → compiler.smt2 → .smt2 → run`
self-hosted round trip to a clean exit — the deletion-readiness signal
for the prelude capability.

## Item 2 — full test_hello smoke test: NOT green (pre-existing blockers)

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev
hello` does not exit 0. The flattened input is 4137 chars / 106 lines
(it inlines all of `stdlib/kernel.ev`). At the per-tick solve cost
documented in wave-4c blocker 5 (~40–60 s/char-level lex tick over the
200 348-line `compiler.smt2`), a 4137-char input is intractable — the
self-hosted compile does not complete. Independently, the flattened
source still hits wave-4c blockers 1 (≈70 % comment lines, no comment
mode in the lexer), 2 (`LibCall`'s `Seq(LibArg)` nested field type +
`__SeqOf_LibArg` payload), and 3 (parametrized `Build*` sugar claims +
claim selection). None of these are Blocker A; see
`docs/plans/blocked-grammar-wave4d.md` for the re-prioritised chain.

## Diff scope

- `compiler/compiler.ev` — `prelude` constant added to the EMIT
  assembly; `smtlib` now interleaves it after the manifest.
- `tests/kernel/test_compiler_driver_prelude.ev` — new fixture
  asserting the self-hosted-equivalent emit shape compiles+runs.
- `docs/plans/grammar-wave4d.md`, `docs/plans/blocked-grammar-wave4d.md`.

`compiler.smt2` rebuilt via bootstrap: 200 348 lines (was 200 341;
+7 for the prelude string).
