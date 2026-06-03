# compiler.ev grammar — wave 4b (multi-top-level + L3 nesting + semantic diff)

Status: **items 1 + 2 + 3 landed; item 4 (slot-bind) still blocked; item 5
(deletion-readiness smoke test) NOT green — blocked on a capability chain
dominated by the Seq/effects ENCODING mismatch (see
`docs/plans/blocked-grammar-wave4b.md`).**

Cites: `docs/plans/grammar-wave4.md` (what wave 4 left), the wave-4
slot-bind blocker `docs/plans/blocked-grammar-wave4-slot-bind.md`, and the
harness it extends `scripts/diff-vs-bootstrap.sh`.

## Item 1 — multi-top-level-item dispatch (DOMINANT) ✓

Wave 3.5 dispatched a SINGLE top-level item by head token (`KwEnum` →
enum machine; otherwise a claim). Wave 4b turns phase 2 of
`compiler/compiler.ev` into a LOOP over a sequence of top-level items via a
new sub-mode `pmode`:

- **0 DISPATCH** — peek the head of the remaining stream `_items`:
  `KwEnum` → enter the enum sub-machine (pmode 1); any other non-empty head
  → enter the claim sub-machine (pmode 2); empty → `all_done` → EMIT.
- **1 ENUM** — accumulate ONE enum's `(declare-datatypes …)` block into
  `eacc`, **stopping at the next top-level keyword** (`enum`/`claim`/`type`/
  `fsm`/`schema`) or end of stream instead of draining the whole list, then
  return to DISPATCH with `items` advanced to the boundary keyword onward
  (the keyword is NOT consumed — the next dispatch picks it up).
- **2 CLAIM** — the wave-1..4 membership walk over the item, unchanged;
  on completion returns to DISPATCH.

The emitted unit is `manifest header · accumulated enum datatype blocks
(_eacc) · is_first_tick decl + claim declares/asserts (_out)`, mirroring
bootstrap's datatypes-before-declares ordering.

The key change to the enum machine: `at_boundary` (and a new `enum_done`)
now treat a top-level keyword as an enum terminator, and the enum state
carries (`plist`/`ephase`/`ename`/`body`/`first_v`/`cur_name`/`fcount`/
`fA..fC`) are gated on per-item `enter_enum`/`in_enum_run` signals (re-init
each item) rather than the old one-shot `entering_parse ∧ is_enum_program`.

Fixture: `tests/kernel/test_compiler_driver_multi_toplevel.ev` — two enum
decls (`Color`, `Dir`) followed by a `claim foo` with one membership.
Verified: both `(declare-datatypes …)` blocks precede the claim's
`(declare-fun x () Int)` / `(assert (= x 5))`, exit 0.

## Item 2 — semantic diff harness ✓

`scripts/diff-vs-bootstrap.sh` grew a `--semantic` flag (byte mode stays the
default for backward-compat). With `--semantic`, instead of diffing the two
`.smt2` files it RUNS both on the kernel and compares the observable
behaviour (stdout + exit code). The intent (coordinator decision) was that
this resolves wave-4 gap #2 (Seq encoding: `seq.++` vs `(Array Int …)`) and
gap #3 (`max-effects`) at the test layer — different bytes, same behaviour.

**Caveat discovered this wave:** semantic mode does NOT actually rescue
those two gaps, because the kernel cannot RUN the self-hosted seq-theory
encoding at all (it errors before producing output — see the blocker doc).
The flag is still the correct test bar once the self-hosted compiler emits
the array+len encoding; it is just not sufficient on its own to flip the
smoke test.

## Item 3 — L3 constructor nesting ✓

`compiler/translate_ctor.ev` gained `RenderExprL3` (children are L2),
following the existing depth-unrolled L0/L1/L2 pattern; `RenderExprToks`
now aliases L3. This renders the canonical effect-builder shape:

```
LibCall("libc", "puts", ⟨ArgStr("hi")⟩)
  →  (LibCall "libc" "puts" (seq.unit (ArgStr "hi")))
```

ctor(L3) → Seq(L2) → ctor(L1) → atom(L0). Fixture:
`tests/kernel/test_compiler_driver_ctor_l3.ev`.

Chosen the depth-unrolled extension (one more level) over the work-stack
walker because the named fixture only needs L3. The constraint count grows
~6× per level: the L3 driver is ~5745 residual assertions and the L3
fixture takes ~6.6 s on Z3. **This is the stopping point for unrolling.**
The full flattened `test_hello` effects literal `⟨LibCall(…), Exit(0)⟩` is
L4 (an outer Seq of an L3 ctor), and arbitrary corpus depth is unbounded —
both need the token work-stack walker (the `SeqConcatStep` /
`ArithTranslateStep` pattern). Going to L4 by unrolling would be ~35 s and
risks the test timeout, so it is explicitly deferred.

## Item 4 — slot-bind composition: still blocked

Item 1's `pmode` loop makes the **claim registry** prerequisite tractable
(collect claim decls during DISPATCH), which was the dependency the wave-4
slot-bind note flagged. But the body-inline-with-substitution core still
needs name-rewrite / α-rename machinery and robust nested parsing that this
wave did not build. See the updated
`docs/plans/blocked-grammar-wave4-slot-bind.md` and the wave-4b blocker doc.

## Item 5 — deletion-readiness smoke test: NOT green

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
does not exit 0. The remaining gap is a CHAIN, not a single grammar miss,
and the dominant link is the Seq/effects ENCODING mismatch (the kernel
cannot run the self-hosted seq-theory output at all). Full analysis,
empirically verified, in `docs/plans/blocked-grammar-wave4b.md`.

## Verification

- `scripts/run-kernel-tests.sh`: **93 kernel tests, 0 failed** (was 91; +2
  new fixtures). All prior fixtures byte-identical — they are self-contained
  copies of the per-pass FSMs and do NOT import `compiler/compiler.ev`, so
  the driver rewrite cannot perturb them.
- `scripts/build-compiler-smt2.sh`: green; `compiler.smt2` (30808 lines)
  builds via bootstrap from the rewritten `compiler/compiler.ev`.

## No frozen files touched

No `bootstrap/`, `kernel/`, `stdlib/`, or Python. Diff is
`compiler/compiler.ev` + `compiler/translate_ctor.ev` +
`scripts/diff-vs-bootstrap.sh` + two new `tests/kernel/*` fixtures + this
doc + the wave-4b blocker doc.
