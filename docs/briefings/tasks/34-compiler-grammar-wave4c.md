# Task: compiler.ev grammar — wave 4c (Array+len Seq lowering + max-effects)

## Why

Wave 4b landed multi-top-level dispatch, semantic diff harness,
and L3 ctor nesting. The smoke test still fails because:

> *"The self-hosted compiler emits Z3 seq-theory effects
> (`(Seq Effect)`/`seq.++`/`max-effects=0`), but the kernel's tick
> loop only consumes `(Array Int Effect)+effects__len+max-effects`."*

Verified empirically by wave 4b — even a hand-written seq.++
encoded `test_hello` makes the kernel error with `unknown constant
last_results__len` before any output.

This task ports bootstrap's encoding choice for Seq state fields:
**emit `(Array Int T)` + `__len` instead of `seq.++` for
top-level `Seq(T)` memberships**, and derive `max-effects` from
the body. With this lands, `compiler.smt2` produces kernel-runnable
output for `test_hello`-shaped programs.

This is **bridge work** — porting bootstrap's encoding for now to
get deletion-ready. The longer-term design (`Seq(T)` as interface
with multiple backings) is captured in `docs/plans/ideas.md`
§"`Seq(T)` as interface, multiple backings."

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures),
`scripts/diff-vs-bootstrap.sh` (only if needed), and `docs/`. No
`bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-wave4b.md` — what just landed and what's
   left.
3. `docs/plans/blocked-grammar-wave4b.md` — the precise blocker
   list, especially item 0.
4. **`bootstrap/runtime/src/translate/exprs/seq_eq.rs`** — the
   bootstrap encoding you're porting. Specifically the
   `translate_seq_rhs_eq` path and `translate_seq_value`.
5. **`bootstrap/runtime/src/emit.rs`** — for how `max-effects` is
   derived from the body (look for `discover_max_effects` or
   similar).
6. `compiler/translate_seq.ev` — the current seq.++ pass to
   replace.
7. `compiler/compiler.ev` — the canonical driver where
   max-effects is emitted in the manifest header.
8. `tests/kernel/test_hello.ev` — the smoke-test target.

Cite #4 and #5 in your report.

## Scope

### Item 1: Array+len encoding for top-level Seq memberships

When `compiler.ev` sees a top-level membership like
`effects ∈ Seq(Effect) = ⟨...⟩` (or any `xs ∈ Seq(T) = ⟨...⟩`),
emit bootstrap's encoding:

- `(declare-fun xs () (Array Int T))`
- `(declare-fun xs__len () Int)`
- `(assert (= xs__len N))` where N = literal length
- `(assert (= (xs 0) <elem0>))` ... `(assert (= (xs (N-1)) <elemN-1>))`

For `xs = a ++ ⟨e⟩`, the encoding is similar but uses the
concat semantics bootstrap implements.

For empty `⟨⟩`, length is 0 and no element asserts.

Look at bootstrap's `translate_seq_value` for the exact form.
The kernel reads this directly because tick.rs has
`read_seq_var` (added in task #21) that handles
`(Array Int T)` + `__len` form.

### Item 2: max-effects derivation in the manifest

Bootstrap auto-derives `max-effects` from the body by scanning
all `effects = <literal>` and `effects = ... ++ <literal>`
constraints and taking the maximum effect Seq literal size.
`compiler.ev` currently hardcodes `max-effects = 0`.

Port the derivation:
- Walk the body's BodyItemList.
- For each `effects ∈ Seq(Effect) = <expr>` (or `effects = <expr>`),
  if the RHS is or contains a literal Seq `⟨...⟩`, count its length.
- Track the maximum.
- Emit `;; manifest: max-effects = <N>` instead of `= 0`.

For simple cases (single literal effects assignment),
max-effects = literal length. For guarded cases (ternary), use
the max of the branches. For `++`-composed cases, sum the literal
operand lengths.

### Item 3: smoke-test signal

After items 1 and 2:

```bash
scripts/build-compiler-smt2.sh
scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello
```

**This is the deletion-readiness signal.** If exit 0, we are
ready for the cutover (`EVIDENT_SELF_VIA_SMT2=1 ./test.sh`).

If exit non-zero, identify what's still blocking. The wave 4b
blocker list named candidates beyond item 0: lexer comment
stripping, nested enum field types, parametrized-claim skip.
Document precisely which surfaces, in a new blocker doc.

## Acceptance

1. compiler.ev's Seq translator emits Array+len for top-level
   Seq memberships.
2. compiler.ev derives max-effects from the body.
3. **Smoke test from item 3 exits 0** OR a precise blocker doc
   identifies the next gap.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. All previous-wave fixtures still pass byte-identical (their
   Seq usage was likely in-expression literals; if they break,
   investigate — Array+len applies to STATE fields, not all
   Seqs).
6. Diff scoped to `compiler/*.ev` + new fixtures +
   `docs/plans/grammar-wave4c.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling other items from wave 4b blocker list (lexer comments,
  nested enum, etc.) — those are subsequent waves.
- Implementing the Seq-as-interface design (that's post-deletion).

## Known gotchas (carry-over)

- Op/Token/Expr variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- The match-on-composed-MArm constraint silently drops; use
  inline token assembly when composing match (wave 3.5 finding).

## Reporting back

- Branch pushed (`agent-34-compiler-grammar-wave4c`).
- Per-item status (1, 2, 3).
- **Smoke test #3 result — the headline.**
- Test count delta (current: 93).
- Cite docs.

Be terse.
