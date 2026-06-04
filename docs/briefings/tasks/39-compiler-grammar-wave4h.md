# Task: compiler.ev grammar — wave 4h (in-ctor Seq-value cons encoding; Blocker 2c)

## Why

Wave 4g closed parsing + datatype facets for nested enum payloads
(Blocker 2a, 2b) and parametrized-claim skip (Blocker 3). The
self-hosted compile of the full `test_hello` now succeeds (1436
ticks, exit 0) and the emitted `.smt2` matches bootstrap's in EVERY
WAY EXCEPT ONE renderer line — `docs/plans/blocked-grammar-wave4g.md`
describes it precisely.

`test_hello`'s effects literal:

```evident
effects ∈ Seq(Effect) = ⟨LibCall("libc", "puts", ⟨ArgStr("hello world")⟩), Exit(0)⟩
```

Self-hosted emits (sort mismatch — kernel rejects):

```
(LibCall "libc" "puts" (seq.unit (ArgStr "hello world")))
```

Bootstrap emits (right):

```
(LibCall "libc" "puts" (__Cell_LibArg (ArgStr "hello world") __Empty_LibArg))
```

The `__SeqOf_LibArg` helper is already declared correctly (wave 4g
Item 1b). The renderer `translate_ctor.ev::ListThread3.seq_wrapped`
is element-type-AGNOSTIC; this wave threads element-type context
through so it can emit `__Cell_<T>` / `__Empty_<T>` when the position
is a `Seq` payload.

**After this lands**, `test_hello --semantic` should exit 0 — the
deletion-readiness signal that unblocks flipping `test.sh` to the
self-hosted compiler.

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures), `docs/`.
No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. **`docs/plans/blocked-grammar-wave4g.md`** — the precise blocker
   spec, including bootstrap's element-type-driven renderer it
   names as the porting target.
3. `docs/plans/grammar-wave4g.md` — what just landed (`__SeqOf_<T>`
   helper block, `SeqEltType` lookup, claim selection) and the
   verification evidence.
4. `compiler/translate_ctor.ev` — where `seq_wrapped` lives.
5. `compiler/translate.ev` — `SeqEltType`, `FieldSortName`,
   `SeqHelperBlock` (wave 4g's adds).
6. `compiler/compiler.ev` — the canonical driver; see how it
   captures enum/variant decls.
7. Bootstrap's `runtime/src/runtime/register_enums.rs::generate_internal_cons_helpers`
   and its emit path for the Seq VALUE — find the exact function
   that knows the element type at value-emit time. Port that
   threading.

Cite #2 in your report.

## Scope

### Item 1: variant signature table

The driver already parses every `enum E = Var1(T1, T2, ...) | ...`
declaration. Build a lookup table the renderer can query:

```
{ ctor_name + arg_index → field_type_text }
```

where `field_type_text` is the parsed token (e.g. "Int", "String",
"Seq(LibArg)").

For nested constructors `Outer(_, Inner(...))`, the table is queried
per node — the outer node sees `Outer`'s arg-i type; the recursive
descent into `Inner(...)` uses `Inner`'s arg-j type. The driver
already threads ctor names through the render walk; extend that
thread to carry the field's expected type alongside.

### Item 2: element-type-aware seq render

`translate_ctor.ev::ListThread3.seq_wrapped` (and its L1/L2 peers,
if they exist as separate copies) gain an element-type parameter.
When the expected field type is `Seq(T)`, the renderer emits the
cons form:

```
(__Cell_<T> e1 (__Cell_<T> e2 ... __Empty_<T>))
```

When the position is NOT a `Seq(T)` payload (e.g. a bare
expression-level Seq literal), the existing `seq.unit`/`seq.++`
form stays. The choice keys off whether the renderer was given a
field-type hint AND that hint matches `Seq(<X>)`.

For empty Seq literals `⟨⟩`, emit `__Empty_<T>` instead of an
empty `seq.++` / `seq.unit` form.

### Item 3: thread the hint through the ctor walk

`RenderExprL1`/`L2`/`L3` (or whichever names compiler.ev uses) need
to pass each ctor argument's expected type-text down to its child
renderer. The expected-type text comes from the table built in
Item 1.

For the OUTER `effects = ⟨…⟩` assignment, the expected type is
`Seq(Effect)`, so the outer renderer also uses the cons form
(`__Cell_Effect`/`__Empty_Effect`). Wave 4g built `__SeqOf_Effect`
implicitly already? — verify and add the helper-block emission for
each `__SeqOf_<T>` actually referenced (not just `__SeqOf_LibArg`).

### Item 4: smoke test (the headline)

After items 1-3:

```bash
scripts/build-compiler-smt2.sh
scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello
```

**Expected: exit 0.** That is the deletion-readiness signal.

If still non-zero, identify the remaining blocker precisely
(byte diff, kernel error, whatever surfaces) and document it in
`docs/plans/blocked-grammar-wave4h.md`.

### Item 5: test fixtures

Add fixtures that pin the element-type-aware rendering directly,
independent of the smoke test:

- `tests/kernel/test_compiler_driver_seq_value_cons.ev` —
  renders a `Seq(LibArg)` value as `__Cell_LibArg` cons form.
- `tests/kernel/test_compiler_driver_seq_value_empty.ev` —
  renders `⟨⟩` as `__Empty_<T>`.

Both should pass byte-identical to bootstrap.

## Acceptance

1. Element-type-aware `seq_wrapped` (or its successor) emits
   `__Cell_<T>`/`__Empty_<T>` when the position is a Seq payload.
2. Empty `⟨⟩` literals lower to `__Empty_<T>`.
3. **Smoke test from item 4 exits 0** OR a precise blocker doc
   identifies the remaining gap.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. All wave 4g fixtures still pass byte-identical.
6. Diff scoped to `compiler/*.ev` + new fixtures +
   `docs/plans/grammar-wave4h.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- A `LibArg`-hardcoded `seq_wrapped` (the wave 4g doc explicitly
  warns against this — it'd silently mis-render any future
  `Seq(T≠LibArg)` payload).
- Tackling Blocker 4 (L4 work-stack walker) or Blocker 5
  (per-tick solve cost) — structural, kernel-side.

## Known gotchas

- Op/Token/Expr/Enum variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- Match-on-composed-MArm silently drops; use inline token assembly.
- The compiler.smt2 build via bootstrap is ~seconds; the
  test_hello smoke test takes ~14 min — budget accordingly.
- Wave 4g already exposes `SeqEltType` and the enum walk captures
  Seq element types; you may be able to reuse rather than rebuild
  the table for Item 1.

## Reporting back

- Branch name pushed (`agent-39-compiler-grammar-wave4h`).
- Items 1-3 status.
- **Item 4 smoke-test result — the headline.**
- Test count delta (current: 98).
- Cite docs.

Be terse.
