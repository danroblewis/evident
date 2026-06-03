# compiler.ev grammar — wave 4c (Array+len Seq lowering + max-effects)

Status: **items 1 + 2 landed and proven end-to-end on the real
`compiler.smt2`; item 3 (full `test_hello` smoke test) NOT green — but the
DOMINANT blocker (wave-4b blocker 0, the Seq/effects encoding) is now
RESOLVED. The chain advanced one link: the next surface is the emit
prelude (`Result` / `last_results` / `last_results__len` decls). See
`docs/plans/blocked-grammar-wave4c.md`.**

Cites the bootstrap encodings ported: `bootstrap/runtime/src/translate/
exprs/seq_eq.rs` (`translate_seq_lit_for_var` / `translate_seq_value`) and
`bootstrap/runtime/src/emit.rs` (`build_manifest` / `DEFAULT_MAX_EFFECTS` /
`effects_seqlit_length` / `discover_state_fields`). Extends wave 4b
(`docs/plans/grammar-wave4b.md`, `docs/plans/blocked-grammar-wave4b.md`).

## Item 1 — Array+len encoding for top-level Seq memberships ✓

Wave 4b's `compiler.ev` lowered every `Seq` to Z3 sequence theory
(`(Seq T)` / `seq.++` / `seq.unit`). The kernel's tick loop cannot consume
that (it reads `<name>__len` + `(select <name> i)` over an `(Array Int T)`
— `kernel/src/tick.rs` `read_seq_var`; blocker-0 in
`docs/plans/blocked-grammar-wave4b.md`). This wave ports bootstrap's
`(Array Int T) + __len` lowering for **top-level Seq STATE fields**:

- New claim **`SeqArrayBlock`** (`compiler/translate_ctor.ev`): given the
  token list starting at `⟨`, a var name, and the element SMT type, it
  renders each element FLAT and emits
  ```
  (declare-fun nm () (Array Int T))
  (declare-fun nm__len () Int)
  (assert (= nm__len N))
  (assert (= (select nm 0) <e0>)) … (assert (= (select nm N-1) <eN-1>))
  ```
  Each element is rendered with `RenderExprToks` (= L3), so a
  `⟨L3-ctor, …⟩` outer literal is handled at element granularity. **This
  incidentally dissolves wave-4b blocker 4** (outer-Seq "L4" nesting) for
  the effects channel: the outer Seq no longer costs a depth-unroll level.

- **`CtorMembershipStep`** (`compiler/parse_body_ctor.ev`): the compound
  `Seq(T)` path now dispatches to `SeqArrayBlock`; the scalar ctor path
  (`e ∈ Effect = Exit(0)`) is unchanged (a scalar Effect is not a
  state-field Seq, so any nested `Seq` payload keeps its `seq.unit` form).
  Also excludes `effects` / `last_results` from the manifest state-fields
  (mirrors `emit.rs:500` `discover_state_fields`).

- **`SeqMembershipStep`** (`compiler/parse_body_seq.ev`): `Seq(Int)`
  literals → `SeqArrayBlock`; `base ++ ⟨e⟩` → `(store base (+ base__len 0)
  e)` + `(= nm__len (+ base__len 1))` (bootstrap `translate_seq_value`'s
  `++` arm); `#src` → the companion `src__len` const (not `seq.len`).

In-expression Seq literals (e.g. a ctor's `Seq` payload) keep Z3 seq
theory via the unchanged `RenderExprL0..L3`; Array+len applies only to the
OUTER state field. The seq-theory `RenderSeqUnit` / `SeqLenSmtlib` helpers
in `parse_body_seq.ev` are now unused there but retained.

## Item 2 — max-effects derivation ✓ (with a correction to the task spec)

The task said "bootstrap auto-derives max-effects from the body."
**Empirically false:** `emit.rs:25,134` uses a fixed constant
`DEFAULT_MAX_EFFECTS = 16` (the reference `test_hello.smt2` carries
`max-effects = 16`). We implement the DERIVATION the task asked for
instead — `compiler.ev` now carries an `mxe` accumulator that tracks the
maximum effects-literal length seen in the body (via `CtorMembershipStep`'s
new `eff_len` output, nonzero only for an `effects ∈ Seq(Effect) = ⟨…⟩`
literal) and emits `;; manifest: max-effects = <N>`. Both are
kernel-correct as long as the value ≥ the runtime effects length, which a
single literal binding guarantees. Derivation also minimises fixture churn:
non-effects programs derive 0 (byte-identical to the old hardcoded line).

**Caveat (documented for the cutover):** a tight per-tick bound is unsafe
for a program that GROWS effects across ticks via `effects = _effects ++
⟨…⟩` — there the constant-16 headroom is needed. No current corpus program
does this on `effects`; flagged in the blocker doc.

## Item 3 — deletion-readiness smoke test: NOT green, but blocker 0 cleared

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
still does not exit 0 (full `test_hello` inlines `stdlib/kernel.ev` and
hits wave-4b blockers 1/2/3 — comments, the `Seq(LibArg)` nested enum
field, parametrized-claim skip + claim selection — plus blocker 5's
~40 s/compile cost).

**But the dominant blocker (0) is proven RESOLVED.** Driving the REAL
self-hosted compiler (`kernel + compiler.smt2`) on a minimal
comment-free, single-claim, simple-enum input:

```
enum Effect = Exit(Int)
claim hello
    effects ∈ Seq(Effect) = ⟨Exit(0)⟩
```

emits (≈40 s, blocker 5):

```
;; manifest: state-fields =
;; manifest: max-effects = 1
(declare-datatypes ((Effect 0)) (((Exit (Exit__f0 Int)))))
(declare-fun is_first_tick () Bool)
(declare-fun effects () (Array Int Effect))
(declare-fun effects__len () Int)
(assert (= effects__len 1))
(assert (= (select effects 0) (Exit 0)))
```

— bootstrap's exact kernel-runnable Array+len form, `max-effects` DERIVED
(=1), `effects` excluded from state-fields. Prepending the emit prelude
(`Result` datatype + `last_results` + `last_results__len`, which bootstrap
hand-writes and the self-hosted EMIT phase does NOT yet produce — the new
NEXT blocker) makes the program **run on the kernel to exit 0.** That round
trip is the proof that wave-4b blocker 0 is closed end-to-end.

## Verification

- `scripts/run-kernel-tests.sh`: **94 kernel tests, 0 failed** (was 93; +1
  new fixture `test_compiler_driver_effect_array`), green in all 3
  functionizer modes (default / `EVIDENT_FUNCTIONIZE_JIT=0` /
  `EVIDENT_FUNCTIONIZE=0`).
- `scripts/build-compiler-smt2.sh`: green; `compiler.smt2` builds via
  bootstrap (200 341 lines — up from wave-4b's 30 808; `SeqArrayBlock`
  composes `RenderExprToks` three times).
- Updated fixtures (encoding changed from seq.++ to Array+len):
  `test_compiler_driver_seq`, `test_compiler_driver_canonical_seq`,
  `test_compiler_driver_effect_seq`. New: `test_compiler_driver_effect_array`
  (proves derivation + Array+len + state-fields exclusion through the full
  wave-4b pmode driver).

## No frozen files touched

No `bootstrap/`, `kernel/`, `stdlib/`, or Python. Diff is
`compiler/translate_ctor.ev` + `compiler/parse_body_ctor.ev` +
`compiler/parse_body_seq.ev` + `compiler/compiler.ev` + 3 updated
`tests/kernel/*` fixtures + 1 new fixture + this doc + the wave-4c blocker
doc.
