# Task: compiler.ev grammar — wave 4 (constructor application + slot-binding composition)

## Why

Waves 1, 2, 3, 3.5 brought `compiler/compiler.ev` from MVP (one
membership, integer pin) to handling multi-membership +
arithmetic + ternary + comparisons + bool ops + strings +
state-carry + enum decl + match + matches + Seq decls/literals.

But the cutover session (task #30) surfaced the final blocker:
**`compiler.ev` cannot yet compile real kernel-runnable `.ev`
files** because they all have shapes like:

```evident
effects ∈ Seq(Effect) = ⟨LibCall("libc", "puts", ⟨ArgStr("hi")⟩), Exit(0)⟩
```

The grammar gaps are:
- **Constructor application**: `Exit(0)`, `LibCall("libc", "puts", ⟨…⟩)`,
  `ArgStr("hi")`, `Ok(5)` — an enum variant being *applied* with
  arguments in expression position. Wave 3 added enum *declarations*;
  wave 4 adds enum *use*.
- **Nested Seq literals** containing constructor applications:
  `⟨LibCall(…), Exit(0)⟩` (a Seq(Effect) whose elements are
  constructor calls), and `⟨ArgStr("hi"), ArgInt(0)⟩` (a Seq(LibArg)
  inside a constructor's payload).
- **Slot-binding composition** (`Stack(depth ↦ depth, is_init ↦ is_first_tick, …)`),
  used in FTI tests and elsewhere. Composition mechanism #4 in
  CLAUDE.md.

Once wave 4 lands, `compiler.smt2` should compile the kernel test
corpus, the cutover demo (`EVIDENT_SELF_VIA_SMT2=1 ./test.sh`)
becomes meaningful, and bootstrap can be deleted.

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures), and
`docs/`. No `bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-coverage-survey.md` — note wave-4 entries
   and which ones are required for the kernel corpus.
3. `docs/plans/grammar-wave3.5.md` — what just landed.
4. `compiler/compiler.ev` — the canonical driver to extend.
5. `compiler/parser.ev` — for the AST.
6. `compiler/translate*.ev` — existing passes you may extend or
   compose.
7. **`tests/kernel/test_hello.ev`** — the canonical "smallest real
   kernel program" target. Your fixtures should compile something
   shaped like this.
8. **`tests/kernel/test_fti_stack.ev`** — uses slot-binding
   composition (`Stack(depth ↦ depth, …)`). Reference for item 3.
9. `bootstrap/runtime/src/translate/exprs/enums.rs` (if it exists)
   — for how bootstrap lowers constructor application to SMT-LIB.

Cite #2, #3, and #7 in your report.

## Wave 4 scope

### Item 1: constructor application (THE blocker)

In expression position, `Ctor(arg1, arg2, …)` should be parsed
and lowered. SMT-LIB form: enum variants become datatype
constructors, so `LibCall("libc", "puts", ⟨ArgStr("hi")⟩)` lowers
to `(LibCall "libc" "puts" (seq.unit (ArgStr "hi")))` — the variant
name as the function head, payload args as positional arguments.

Note the parser may already produce `ECall(name, args)` for this
(check first). The translator needs to recognize when `name` is
an enum constructor and emit the datatype constructor application.

Test fixture: `tests/kernel/test_compiler_driver_ctor_app.ev` —
compile a fragment like `e ∈ Effect = Exit(0)` and verify
`(assert (= e (Exit 0)))`.

### Item 2: nested Seq literals with constructor applications

This may be a free win after Item 1 — the Seq literal walker
already exists; what was missing was the constructor application
in element position. Confirm by:

Test fixture: `tests/kernel/test_compiler_driver_effect_seq.ev` —
compile `effects ∈ Seq(Effect) = ⟨Exit(0), Exit(1)⟩` and verify
`(assert (= effects (seq.++ (seq.unit (Exit 0)) (seq.unit (Exit 1)))))`.

If this works after Item 1, you're done with item 2. If it doesn't,
extend the Seq pass to compose constructor application.

### Item 3: slot-binding composition `Claim(slot ↦ value)`

The pattern from `test_fti_stack.ev`:

```evident
Stack(depth ↦ depth, prev_depth ↦ _depth, is_init ↦ is_first_tick, pushing ↦ pushing, popping ↦ popping)
```

This is calling the `Stack` claim with each of its first-line
params bound to a value in the caller's scope. Composition
mechanism #4 in CLAUDE.md.

Bootstrap handles this via name-match composition (the `↦` arrows
explicitly name which parameter receives which value). The
compiler.ev driver needs to:
- Recognize `Identifier ( id ↦ expr, … )` as a composition site.
- Emit constraints that bind the callee's params to the caller's
  exprs.

This is more involved than items 1+2; if you find it pulls in
too much (e.g., requires a full subschema-inline pass), document
the scope blow-up in a blocker doc and STOP after items 1+2 —
those alone make most simple kernel programs compileable.

Test fixture (if item 3 lands): `tests/kernel/test_compiler_driver_slot_bind.ev`.

## Acceptance

1. Constructor application works in expression position.
2. Nested Seq literals with constructor applications work.
3. (Stretch) Slot-binding composition works.
4. At least 2 new test fixtures (item 1 + item 2). Item 3 is
   stretch — if it lands, add a 3rd; if not, write
   `docs/plans/blocked-grammar-wave4-slot-bind.md`.
5. **Smoke test (the deletion-readiness signal):**
   ```
   scripts/build-compiler-smt2.sh
   scripts/diff-vs-bootstrap.sh tests/kernel/test_hello.ev main
   ```
   This should exit 0 (clean diff). If it does, we are ONE STEP
   from bootstrap deletion. If it doesn't, identify the remaining
   grammar gap.
6. All previous-wave fixtures still pass byte-identical.
7. `./test.sh` is fully green in all 3 functionizer modes.
8. Diff scoped to `compiler/*.ev` + new tests + new
   `docs/plans/grammar-wave4.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling quantifiers, generics, records, subclaim (those are
  wave 4b / wave 5; not in this scope).
- Implementing things item-3 needs (real subschema inline,
  per-tick scope resolution) if they balloon the task.

## Known gotchas

- Op/Token/Expr variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- The work-stack recursive-walk pattern is the canonical shape.
- The MArm-composition bug from wave 3.5: if you compose a
  claim that takes an enum-typed parameter, the constraint may
  silently drop. Workaround: inline token-based assembly. See
  grammar-wave3.5.md.

## Reporting back

- Branch pushed (`agent-32-compiler-grammar-wave4`).
- Per-item status (1, 2, 3 — with honest "abandoned" categorisation
  if any).
- Test count delta (current: 88).
- **The acceptance #5 smoke test result** — this is the most
  important signal in the report.
- Cite docs.

Be terse.
