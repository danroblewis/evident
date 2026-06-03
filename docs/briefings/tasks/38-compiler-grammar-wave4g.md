# Task: compiler.ev grammar — wave 4g (nested enum payload types + parametrized-claim skip)

## Why

Wave 4f (LEX bulk-skip) landed and gave us 2.8× wall on test_hello
+ 8.2× on comment-heavy inputs. The smoke test is now feasible to
run (~14 min vs >40 min). Remaining gaps from
`docs/plans/blocked-grammar-wave4d.md` blocker chain after wave 4d:

- Blocker 1 (lexer comment stripping): ✓ closed by wave 4f.
- Blocker 2: **nested enum field types** like `Seq(LibArg)`.
- Blocker 3: **parametrized-claim skip + claim selection**.
- Blocker 4: L4 via work-stack walker (structural; out of this wave's scope).
- Blocker 5: per-tick solve ceiling (structural; out of scope).

This wave tackles blockers 2 and 3 — the remaining mechanical
grammar gaps. After this lands, the smoke test should be much
closer to a clean exit 0 (or surface only structural blockers 4/5,
which are kernel-side work).

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures), and
`docs/`. No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/blocked-grammar-wave4d.md` — the chain you're
   working through. **Read the precise definitions of blockers 2
   and 3.**
3. `docs/plans/grammar-wave4f.md` — the previous wave; understand
   the test_hello smoke-test state.
4. `compiler/parser.ev` — for the AST you'll extend.
5. `compiler/translate.ev` — the enum-decl translator.
6. `compiler/compiler.ev` — the canonical driver.
7. Bootstrap's `runtime/src/parser/` — how it handles claims with
   `<T>` type parameters and nested type params like `Seq(LibArg)`.
8. `tests/kernel/test_hello.ev` — the smoke-test target. Look at
   what shapes appear there that wave 4d/4e/4f didn't handle.

Cite #2 in your report.

## Scope

### Item 1: nested enum field types (Blocker 2)

`stdlib/kernel.ev` defines:

```evident
enum Effect =
    ...
    LibCall(String, String, Seq(LibArg))
    ...
```

The third payload field is `Seq(LibArg)` — a Seq parameterized over
another user-defined enum. The enum translator today probably
handles primitive payload types (String, Int) but fails on
parameterized types in payload position.

Extend `compiler/translate.ev` (or wherever enum decls translate)
to handle nested type parameters in variant payload positions.
The SMT-LIB form is straightforward:

```
(declare-datatypes ((Effect 0))
  (((... 
    (LibCall (LibCall__f0 String) (LibCall__f1 String)
             (LibCall__f2 (Array Int LibArg)))
    ...))))
```

Plus a `LibCall__f2__len` if Seq state-carry rules apply to
payloads. **Check how bootstrap actually emits this — port it
exactly.**

Test fixture: `tests/kernel/test_compiler_driver_enum_seq_payload.ev`.

### Item 2: parametrized-claim skip + claim selection (Blocker 3)

`stdlib/kernel.ev` includes claims like:

```evident
claim BuildPrintln(s ∈ String, eff ∈ Effect)
    eff = LibCall("libc", "puts", ⟨ArgStr(s)⟩)
```

These are *helper claims* that the compiler.ev driver should
**skip** when compiling a target program — only the user's chosen
`main` claim gets compiled. Today the driver may try to emit
declarations/asserts for every claim in the input, causing
duplicate or wrong output.

**Skip non-target claims.** When compiler.ev is invoked with
`evident emit foo.ev main`, only `claim main` should be
translated into output. Helper claims like `BuildPrintln` get
parsed but their bodies don't emit declares/asserts in the
output.

(They DO get inlined into `main`'s body via names-match
composition during translation, but that's a different mechanism
— bootstrap's `inline` pass handles it. Verify by inspecting
bootstrap's emit pipeline.)

Test fixture:
`tests/kernel/test_compiler_driver_claim_selection.ev`.

### Item 3: smoke test

After items 1 + 2, run:

```bash
scripts/build-compiler-smt2.sh
scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello
```

Expected: exit 0 (deletion-ready signal) OR a precise blocker
doc identifying remaining gaps.

If still non-zero, identify what's blocking now. Per the chain,
likely candidates are blockers 4 (L4) and 5 (per-tick solve
ceiling). Document precisely.

## Acceptance

1. Nested enum payload types (e.g. `Seq(LibArg)`) lower correctly.
2. Non-target claims are skipped from output emission (target
   claim only).
3. **Smoke test exits 0** OR a precise blocker doc identifies the
   next gap (and the previous blocker chain advances).
4. `./test.sh` is fully green in all 3 functionizer modes.
5. All previous-wave fixtures still pass byte-identical.
6. Diff scoped to `compiler/*.ev` + new fixtures +
   `docs/plans/grammar-wave4g.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling blockers 4 (L4 work-stack walker) or 5 (per-tick solve
  ceiling) — those are kernel-side structural work.
- Implementing the Seq-as-interface design (post-deletion).

## Known gotchas

- Op/Token/Expr/Enum variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- Match-on-composed-MArm silently drops; use inline token assembly.
- The compiler.smt2 build via bootstrap is fast (~seconds), but
  the smoke test takes ~14 min — budget accordingly.

## Reporting back

- Branch pushed.
- Item 1, 2 status.
- **Item 3 smoke-test result — the headline.**
- Test count delta (current: 96).
- Cite docs.

Be terse.
