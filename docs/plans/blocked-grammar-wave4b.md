# Blocked: wave-4b item 5 — the deletion-readiness smoke test

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
does not exit 0. This note records, precisely and in priority order, the
capability chain that still blocks it. The headline is that the dominant
blocker is NOT a grammar gap — it is an **encoding mismatch the semantic
harness cannot paper over**, verified empirically below.

Cites: `docs/plans/grammar-wave4.md` (which framed the Seq encoding and
`max-effects` as "semantically equivalent" — corrected here),
`docs/plans/blocked-grammar-wave4-slot-bind.md`, and
`scripts/diff-vs-bootstrap.sh` (the harness, now `--semantic`-capable).

## What the smoke test actually requires

`flatten-evident.sh tests/kernel/test_hello.ev` is NOT the tidy
`claim hello / effects = ⟨…⟩` two-liner. It inlines all of
`stdlib/kernel.ev` first, so the translation unit the self-hosted compiler
must consume is:

```
-- (≈70 lines of -- comments)
enum LibArg = ArgInt(Int) ArgStr(String) ArgReal(Real)
enum Effect = ReadLine ReadFile(String) WriteFile(String, String)
              LibCall(String, String, Seq(LibArg)) Exit(Int)
enum Result = NoResult IntResult(Int) StringResult(String) … ErrorResult(String)
claim BuildPrintln(s ∈ String, eff ∈ Effect)   eff = LibCall("libc","puts",⟨ArgStr(s)⟩)
claim BuildTime(eff ∈ Effect)                   …
…6 more Build* sugar claims…
claim hello
    effects ∈ Seq(Effect) = ⟨LibCall("libc","puts",⟨ArgStr("hello world")⟩), Exit(0)⟩
```

and bootstrap's `emit … hello` output (the reference) is: the three enum
datatypes (Result/LibArg/Effect, in `(Array Int …)` Seq encoding with a
`__SeqOf_LibArg` cons datatype), `last_results`, `is_first_tick`, then ONLY
the `hello` claim's body (the Build* claims are unreferenced sugar and are
NOT emitted), with `max-effects = 16`.

## Blocker 0 (DOMINANT) — Seq/effects encoding: the kernel can't run seq theory

The self-hosted renderer (`translate_seq.ev` / `translate_ctor.ev`) lowers
`Seq` to Z3 sequence theory: `(Seq Effect)`, `seq.++`, `seq.unit`, and the
manifest carries `max-effects = 0`. The kernel's tick loop does NOT consume
that. It reads the effects channel as bootstrap's encoding:

- `kernel/src/tick.rs:412,1024` — `read_int_const(… "{effects_name}__len")`
- `:413,1025` — clamp the length to `manifest.max_effects`
- decode each element via `(select effects i)` — an `(Array Int Effect)`.
- `:1071` / `:362` — it also pins `last_results__len` and `effects__len`.

So a seq-encoded program has no `effects__len`, no `(Array …)` to `select`,
and a `max-effects` of 0. **Empirically** (hand-written `(Seq Effect)` +
`seq.++` hello, given every other declaration the kernel wants):

```
$ kernel /tmp/seq-hello.smt2
Error: (error "line N column M: unknown constant last_results__len")
exit=1
```

The kernel errors out **before producing any stdout** — it cannot start the
seq-encoded program. Therefore `--semantic` does NOT rescue wave-4 gap #2
(Seq encoding) or gap #3 (`max-effects`): they are not "different bytes,
same behaviour"; the seq encoding has NO behaviour on this kernel.

**Unblock:** the self-hosted compiler must emit the `(Array Int Effect) +
effects__len` encoding (and the `__SeqOf_T` cons datatype for nested Seq
payloads) and derive `max-effects` from the parsed effects literal —
i.e. port bootstrap's `translate/seq` array lowering, replacing the current
seq-theory `translate_seq.ev`. This is the single highest-value next step;
it is a translation-strategy rewrite, not new grammar. (Alternatively the
kernel could grow a seq-theory effects path, but `kernel/` is frozen-by-
approval and the array encoding is the contract in
`docs/plans/kernel-input-spec.md`.)

## Blocker 1 — comment stripping in the consolidated lexer

The flattened unit is ~70% `--` comment lines. The consolidated lexer in
`compiler/compiler.ev` (phase 0) does not skip comments AND emits no
`Newline`/`Indent` token (whitespace, including `\n`, is only a delimiter).
A `--` therefore lexes as two `Minus` tokens followed by the comment words
as `Ident`s, with no line-boundary marker to find the comment's end. The
dispatch then sees a leading `Minus` (not a top-level keyword), mis-routes,
and drops everything.

**Unblock:** add a comment mode to the char-level lexer FSM — on `-` with
lookahead `-`, consume to the next `\n` and emit nothing. (Mechanical;
`stdlib/lexer.ev::IsBlankLineHead` already encodes the `--` recognition.)

## Blocker 2 — enum variants with multiple / nested-type fields

The enum sub-machine handles ≤3 fields whose types are bare `Ident`s. The
`Effect` enum has `LibCall(String, String, Seq(LibArg))` — the third field
type is `Seq(LibArg)` = `Ident(Seq) LParen Ident(LibArg) RParen`, which the
e6/e7 field-type states mis-parse (they read `Seq` as the type and get
stuck on the `(`). `WriteFile(String, String)` (two fields) is fine, but
the nested compound field type is not.

**Unblock:** extend the enum field-type reader to accumulate a compound
type string (reuse `parser.ev::TypeTokenText` + `BracketDelta`, the bracket-
depth accumulator that already exists for exactly this).

## Blocker 3 — parametrized claims + claim selection

The `Build*` claims carry first-line params `claim BuildPrintln(s ∈ String,
eff ∈ Effect)` and composition bodies. The claim sub-machine's "drop 2 head
tokens then walk memberships" assumes `<kw> Ident` followed immediately by
memberships; a `(` after the name is unhandled. Worse, the driver currently
emits EVERY claim it walks, but the reference emits ONLY the target claim
`hello` (the sugar claims are unreferenced). The self-hosted compiler has no
claim-name argument (it reads `/tmp/compiler-input.ev`), so it cannot today
know which claim is the entry point.

**Unblock:** (a) parse-and-skip first-line params; (b) a claim-selection
mechanism — either take the target claim name as input (a second ReadFile /
env), or emit the LAST top-level claim (the entry point after inlined
sugar), or build the claim registry (now tractable via item 1's DISPATCH
loop) and emit only the referenced claim.

## Blocker 4 — L4+ expression nesting

The full effects literal `⟨LibCall("libc","puts",⟨ArgStr("hello world")⟩),
Exit(0)⟩` is L4 (outer Seq of an L3 ctor). The renderer is L3 (wave 4b).
Unrolling to L4 is ~35 s on Z3 (×6 the L3 cost) and risks the test timeout.

**Unblock:** replace the depth-unrolled `RenderExprL0..L3` with a token
work-stack walker (the `WorkItem`/`WorkList` pattern in `parser.ev`,
already used by `translate_arith.ev`'s recursive walker) — arbitrary depth,
linear cost.

## Blocker 5 — per-tick solve cost of the depth-unrolled renderer

Observed while running the smoke test: `kernel compiler.smt2 < flat` on the
flattened `test_hello` (~3 KB) did not terminate in 20+ minutes. The driver
re-solves its ENTIRE body every tick, and with `RenderExprL3` composed in
(~5.7k residual assertions) each lex tick is seconds of Z3; a multi-KB
source is thousands of char-level lex ticks. The self-hosted compiler is
thus too slow to compile a real file until either (a) the renderer becomes a
work-stack walker whose constraints don't all fire every tick (blocker 4),
or (b) the functionizer extracts the lex/parse steps (it currently reports
`not functionized` on this driver — an output had no covering assignment).
This is downstream of blockers 0–4 but will surface immediately once they
clear, so it belongs on the same map.

## Summary

The smoke test is gated on, in priority order: **(0) the array+len Seq
encoding + `max-effects`** (dominant; the kernel literally cannot run the
current output), then (1) comment stripping, (2) nested enum field types,
(3) parametrized-claim skip + claim selection, (4) L4 nesting via a
work-stack walker. Item 1 (multi-top-level) — the wave-4b deliverable — is
a genuine prerequisite for the whole chain and now works; it is necessary
but far from sufficient. Blocker 0 is the recommended next session: it is a
self-contained translation-strategy port (bootstrap's `translate/seq` array
lowering) and unblocks not just `test_hello` but every kernel-runnable
program the self-hosted compiler will ever emit.
