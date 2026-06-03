# Task: compiler.ev grammar coverage — wave 3 (self-hosting core)

## Why

Wave 1 (multi-membership + arith) and wave 2 (ternary +
comparisons + bool ops + strings + state-carry) have landed. The
survey at `docs/plans/grammar-coverage-survey.md` identifies wave
3 as **the structural blocker wave** — the shapes
`compiler/compiler.ev`'s own source uses that nothing earlier
handles. Once wave 3 lands, `compiler.ev` can plausibly compile
itself (with `scripts/flatten-evident.sh` preprocessing imports).

## Authorisation

You may edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures),
and `docs/`. No `bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-coverage-survey.md` §"Wave 3" — the spec
   you're implementing.
3. `docs/plans/grammar-wave2.md` — what just landed and the
   conventions to follow (e.g., `Op` variants are globally unique
   — pick distinct names).
4. `compiler/compiler.ev` — the driver. Add new dispatch
   branches per new shape.
5. `compiler/parser.ev` — for the AST you'll need to recognise.
6. `compiler/translate.ev`, `translate_match.ev` — existing
   per-pass fixtures you'll compose into the driver.
7. `tests/kernel/test_translate_arith_recursive.ev` and the
   wave-2 fixtures — the test pattern.

Cite #2 and the wave-2 doc.

## Wave 3 scope

### Item 1: `enum` declarations (incl. payload variants)

```evident
enum Color = Red | Green | Blue
enum Result = Ok(Int) | Err(String)
```

Lower to SMT-LIB `(declare-datatypes ((Color 0)) (((Red) (Green) (Blue))))`
etc. Compose existing `compiler/translate.ev` (the C1 pass) into
the driver.

Test: `tests/kernel/test_compiler_driver_enum.ev`.

### Item 2: `match` expression + patterns

```evident
n = match e
    Ok(v)  ⇒ v
    Err(_) ⇒ 0
```

Lower to nested ITE over `((_ is Ctor) e)` testers. Compose
`compiler/translate_match.ev`.

Test: `tests/kernel/test_compiler_driver_match.ev`.

### Item 3: `e matches Ctor(_)` recognizer

```evident
is_ok = e matches Ok(_)
```

Lower to `((_ is Ok) e)`. Sibling of `match` lowering; probably
the same translator code.

Test: `tests/kernel/test_compiler_driver_matches.ev`.

### Item 4: `Seq(T)` + `⟨…⟩` literal + `++` + `#`

```evident
xs ∈ Seq(Int) = ⟨1, 2, 3⟩
ys ∈ Seq(Int) = xs ++ ⟨4⟩
n ∈ Int = #xs
```

Compose `compiler/translate_seq.ev`. SMT-LIB:
`(declare-fun xs () (Seq Int))`,
`(assert (= xs (seq.++ (seq.unit 1) (seq.unit 2) (seq.unit 3))))`,
`(assert (= n (seq.len xs)))`.

Test: `tests/kernel/test_compiler_driver_seq.ev`.

## Acceptance

1. All 4 items work end-to-end via `compiler.ev` (no hardcoded
   AST; real lex+parse).
2. 4 new test fixtures pass.
3. All wave-1 + wave-2 + MVP fixtures still pass byte-identical.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. **Smoke test (proves we're close to self-hosting):**
   ```
   scripts/flatten-evident.sh compiler/compiler.ev > /tmp/flat.ev
   bootstrap/runtime/target/release/evident emit /tmp/flat.ev main -o /tmp/orig.smt2
   ```
   Today this works (bootstrap handles everything). With wave 3,
   you have what `compiler.ev` *itself* uses, so when we later
   build `compiler.smt2`, it can compile `flat.ev` equivalently.
   You don't need to demonstrate that loop closure in this task
   — just confirm bootstrap's emit on the flattened compiler is
   green.
6. Diff scoped to `compiler/*.ev` + new tests + new
   `docs/plans/grammar-wave3.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling wave 4 shapes (quantifiers, generics, records,
  subclaims, full FTI in-driver use).
- Hardcoding ASTs in test fixtures.

## Known gotchas (from waves 1-2)

- `Op`/`Token`/`Expr` enum variant names are **globally unique**.
  If your name collides with an existing one, the parser errors
  "enum variant declared twice." Wave 2 hit this with
  `OpLe`/`OpGe`/`OpNe` (lexer tokens). Pick distinct spellings
  for new variants you add.
- Composition leaks callee body-local names into the caller and
  unifies them by name → silent UNSAT if the names match.
  Prefix your new pass-claim locals to avoid this. Wave 2 used
  `ms_` / `b_` / etc.
- The work-stack recursive-walk pattern (cons-list of `WorkItem`)
  is the canonical shape. Use it.

## Reporting back

- Branch pushed (`agent-29-compiler-grammar-wave3`).
- The 4 new test fixtures' emitted SMT-LIB (1-2 lines each).
- `./test.sh` final line.
- Test count delta (current: 81).
- Confirm the smoke test (bootstrap emit on
  `scripts/flatten-evident.sh compiler/compiler.ev` flattened
  output is green).
- Any new AST variants you added.
- Cite docs.

Be terse.
