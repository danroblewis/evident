# compiler.ev grammar coverage — wave 4 (constructor application + nested Seq literals)

Status: **items 1 + 2 landed; item 3 (slot-binding composition) deferred**
(blocked on out-of-scope infrastructure — see
`docs/plans/blocked-grammar-wave4-slot-bind.md`).

Waves 1–3.5 brought `compiler/compiler.ev` from MVP (one membership,
integer pin) through arithmetic / ternary / comparisons / bool ops /
strings / state-carry / enum decl / match / matches / Seq decls+literals.
Wave 4 adds enum **use** (constructor application) in expression position —
the shape every kernel-runnable `.ev` file's `effects` line is built from:

```evident
effects ∈ Seq(Effect) = ⟨LibCall("libc", "puts", ⟨ArgStr("hi")⟩), Exit(0)⟩
```

## What landed

### Item 1 — constructor application (the blocker)

A variant applied with arguments in expression position now lowers to the
SMT-LIB datatype-constructor application (the variant name is the function
head, payload args are positional):

```
e ∈ Effect = Exit(0)            →  (assert (= e (Exit 0)))
e ∈ Result = Ok(5)              →  (assert (= e (Ok 5)))
e ∈ LibArg = ArgStr("hi")       →  (assert (= e (ArgStr "hi")))
```

Mirrors bootstrap's datatype lowering (head = variant name, operands =
translated payload args), matching the `(declare-datatypes …)` constructors
that wave 3's `translate.ev` already emits.

Fixture: `tests/kernel/test_compiler_driver_ctor_app.ev` — the unmodified
`compiler/compiler.ev` FSM on a constant input, emitting
`(assert (= e (Exit 0)))`.

### Item 2 — nested Seq literals of constructor applications

A `Seq(T)` literal whose elements are constructor applications:

```
effects ∈ Seq(Effect) = ⟨Exit(0), Exit(1)⟩
  →  (assert (= effects (seq.++ (seq.unit (Exit 0)) (seq.unit (Exit 1)))))
```

Fixture: `tests/kernel/test_compiler_driver_effect_seq.ev`.

### New pass: `compiler/translate_ctor.ev` — `RenderExprToks`

A recursive-descent expression renderer over the lexer `TokenList`,
consuming one expression off the front and returning `(out, rest, ok)`.
Handles atoms (`IntLit`/`StringLit`/bare `Ident`), constructor applications
`Ctor(a1, …, a3)`, and Seq literals `⟨e1, …, e3⟩` — the latter two with
arbitrarily-mixed children.

Evident claims can't self-recurse, so the recursion is **depth-unrolled**:
`RenderExprL0` (atoms) ← `RenderExprL1` (ctor/seq of L0) ← `RenderExprL2`
(ctor/seq of L1). `RenderExprToks` exposes L2 — enough for the required
shapes (a constructor of atoms; a Seq of such constructors). The
constraint count grows ~6× per level (3 ctor args + 3 seq elems per level),
so we stop at the depth the required fixtures need; deeper nests are an
explicit limitation (below).

### New pass: `compiler/parse_body_ctor.ev` — `CtorMembershipStep`

A sibling of `MembershipStep` / `SeqMembershipStep`, specialised for a
membership whose RHS is a constructor application or a Seq of constructor
applications. It **self-discriminates** (peeks t3/t4/t5… off `_rem`; `ok`
fires only for a ctor-app or complex-Seq shape), so the driver dispatches to
it via a single `mem_is_ctor = cts_ok` discriminator given **highest
priority** in the selectors. A `name ∈ Seq(T) = ⟨Ctor(…)…⟩` line is also
`mem_is_seq` (compound type), so the ctor step must win — hence the priority
ordering `ctor → match → seq → scalar`.

Crucially this leaves `MembershipStep`, `SeqMembershipStep`,
`MatchMembershipStep`, and every wave-1/2/3/3.5 fixture **byte-identical** —
the ctor shape never matches a non-ctor line, so all prior fixtures take the
exact same path as before.

## Verification

- `./test.sh`: **all phases passed.**
- Kernel tests: **91 (was 88), 0 failed**, green under default /
  `EVIDENT_FUNCTIONIZE=0` / `EVIDENT_FUNCTIONIZE_JIT=1`.
- Isolation (`tests/kernel/test_render_ctor_iso.ev`): `RenderExprToks` on
  hand-built TokenLists → `(Exit 0)`, the 2-element effect Seq, and
  `(LibCall "libc" "puts")`.
- End-to-end through the **actual** `kernel + compiler.smt2`
  (`scripts/build-compiler-smt2.sh` then a single-item ctor program on
  `/tmp/compiler-input.ev`): a multi-membership claim mixing scalar ctor +
  Seq-of-ctor compiles correctly with no bootstrap on the path.

## Acceptance #5 smoke test (the deletion-readiness signal)

```
scripts/build-compiler-smt2.sh                                 # exit 0, 7371-line compiler.smt2
scripts/diff-vs-bootstrap.sh tests/kernel/test_hello.ev hello  # DIFFERS
```

The diff **differs**, and the difference identifies the remaining gap — it
is **not** constructor application (proven working above). It is:

1. **Imports / multi-top-level-item.** `flatten-evident.sh` inlines
   `stdlib/kernel.ev` ahead of the `hello` claim, so the translation unit
   has many top-level items (the `Effect`/`Result`/`LibArg` enum decls,
   `last_results`, the `Build*` sugar claims) before the claim. `compiler.ev`
   still handles exactly **one** top-level item (wave 3.5's documented
   restriction), so it processes only the first and drops the rest. This is
   the dominant blocker and is wave 4b/5 scope.
2. **Seq encoding strategy.** Bootstrap lowers `Seq(Effect)` to an
   `(Array Int Effect)` + `effects__len` pair and `LibArg` payloads to a
   `__SeqOf_LibArg` cons datatype; the self-hosted renderer uses Z3
   sequence theory (`seq.++` / `seq.unit`), matching `translate_seq.ev`.
   These are different encodings of the same surface — equivalence here is a
   translation-strategy decision, not a grammar gap.
3. **`max-effects`.** Bootstrap computes the actual effect count (16);
   the self-hosted manifest hardcodes 0.

None of these is wave-4 scope. Wave 4's deliverable — constructor
application + nested Seq literals — is complete and self-host-proven; the
smoke test now points squarely at multi-item/imports as the next blocker.

## Out of scope (wave 4b / 5)

- **Slot-binding composition** `Claim(slot ↦ value)` (item 3) — blocked on a
  claim registry + body-inline-with-substitution, which require multi-item
  parsing. See `docs/plans/blocked-grammar-wave4-slot-bind.md`.
- Multi-top-level-item files + `import` resolution (the smoke-test blocker).
- Deeper expression nesting (L3+) — e.g. `LibCall("libc","puts",⟨ArgStr("x")⟩)`
  where a ctor arg is a Seq containing a ctor needs L3; the flattened
  `test_hello` effects line is depth ~4. Adding levels is mechanical but the
  constraint count grows ~6×/level; the unbounded fix is the token
  work-stack walk (the `SeqConcatStep`/`ArithTranslateStep` pattern).
- Seq-encoding parity with bootstrap (Array+len vs sequence theory).
- `max-effects` computation from the parsed effects literal.

## No frozen files touched

No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python. Diff is
`compiler/compiler.ev` + new `compiler/translate_ctor.ev` +
`compiler/parse_body_ctor.ev` + three new `tests/kernel/*` fixtures + this
doc + the item-3 blocker note.
