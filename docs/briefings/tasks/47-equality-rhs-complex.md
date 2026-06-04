# Task: `name = <complex expr>` equality assertions (wave 4p)

## Why

This session ran `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` for
the first time with wave 4o's emit fixes. Results:

- **164 claims, 34 failed = 130 / 164 = 79.3% pass rate** under
  the seam vs bootstrap's 164 / 164 pass.
- **Every failure is `unsat_*` claim reporting sat** — that is,
  contradictory constraints that should be UNSAT are being emitted
  as SAT, meaning **constraints are being dropped on the floor**.
- 1h35m wall-clock, parallel runner working cleanly (no race
  contamination, exact one claim marker per claim).

Pattern across failures (sample, full list in
`/tmp/lang-seam-v2.log`):

| Shape                                  | Example                           | Count |
| -------------------------------------- | --------------------------------- | ----- |
| `name = Ctor(args)` payload mismatch   | `e = Exit(0)` vs `e = Exit(42)`   | ~14   |
| `name = (cond ? a : b)` ternary        | `x = (flag ? 7 : 99)`             | ~1    |
| Multi-name range / chain               | `0 < a, b ∈ Int < 10`             | ~3    |
| Composition + chain mismatch           | `chain_via_composition_violates`  | ~2    |
| Record literal equality                | `c = Color(r ↦ 1, g ↦ 2, b ↦ 3)`  | ~3    |
| Match-result equality                  | `r = match … (Ok(v) ⇒ v …)`       | ~1    |
| `matches` predicate                    | `e matches Ok(_)` etc.            | ~2    |
| Multiline payload                      | (variant constraint not enforced) | ~6    |
| Nested record / json shape             | `nested_record_lit`               | ~1    |
| Mutual recursion / enum cross-decl     | `mutual_recursion_mismatch`       | ~1    |

The Wave 4j fix taught the compiler to emit `(assert (= name atom))`
when it sees `name = literal` after a prior `name ∈ Type`. This
wave is the **sibling fix for the right-hand-side being a complex
expression** — constructor calls, ternary, record literals, match
results, `matches` predicates.

After this wave, `--lang` under seam should reach > 95% pass and
unblock the cutover. The remaining gaps (multi-name range edge
cases, mutual recursion) likely have their own focused fixes that
can be separate waves.

## Authorisation

Edit:
- `compiler/*.ev` — esp. parse_body.ev / parse_body_ctor.ev /
  parse_body_match.ev and the dispatch handlers.
- `compiler.smt2` / `sample.smt2` — rebuild after the source change.
- `tests/kernel/*.ev` — fixtures.
- `docs/` — wave doc.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — composition, chained membership.
2. `docs/plans/wave-4j-sample-and-eq-fix.md` — the bare `=` /
   `≠` fix that this wave generalises.
3. `docs/plans/wave-4l-...` and `docs/plans/wave-4k-...` — what
   shape coverage already exists.
4. `docs/plans/wave-4o-fix-sample-emit.md` — what just landed.
5. `/tmp/lang-seam-v2.log` — the actual failure list (in this
   session's working tree).
6. `compiler/parse_body.ev` — `ms_is_bare` and the bare-`=` path
   from 4j. The complex-RHS dispatch lives near here.
7. `tests/lang_tests/test_kernel_enums.ev::unsat_exit_wrong_payload` —
   the simplest example for fixing.

## Scope

### Item 1: payload-ctor equality

`name = Ctor(args)` after a prior `name ∈ EnumType` must emit:

```
(assert (= name (Ctor args...)))
```

Today the compiler emits an empty body for this line (constraint
dropped). The bare-`=` handler from 4j only handles `name = atom`
where atom is an Ident/Int literal.

Find the dispatch in `parse_body.ev` for bare-`=` lines (4j's
`ms_is_bare`). Extend the RHS parser to accept a constructor
application: `<Ident> ( <arg-list> )`. Emit `(= name (<Ident>
<args...>))`.

Fixture: `tests/kernel/test_compiler_driver_eq_ctor.ev`.

### Item 2: ternary equality

`name = (cond ? a : b)`. Emit `(assert (= name (ite cond a b)))`.

Today: dropped same way as ctor.

Fixture: `tests/kernel/test_compiler_driver_eq_ternary.ev`.

### Item 3: record-literal equality

`name = TypeName(field1 ↦ val1, ...)` or positional
`name = TypeName(val1, val2, ...)`.

Emit one assertion per field:
`(assert (= (TypeName__f0 name) val1))` etc.

Or one composite:
`(assert (= name (TypeName val1 val2 ...)))`.

Pick whichever matches bootstrap's emit exactly. Fixture:
`tests/kernel/test_compiler_driver_eq_record_lit.ev`.

### Item 4: match-result equality

`r = match e (Var1(x) ⇒ x ; Var2 ⇒ 0)`. Lower to an `(ite
(_ is Var1) (Var1__f0 e) ...)` chain.

Fixture: `tests/kernel/test_compiler_driver_eq_match.ev`.

### Item 5: `matches` predicate

`e matches Ok(_)` lowers to `((_ is Ok) e)` Z3 datatype recognizer.

`flag = (e matches Ok(_))` lowers to
`(assert (= flag ((_ is Ok) e)))`.

Fixture: `tests/kernel/test_compiler_driver_eq_matches.ev`.

### Items 6-10: the rest

Multi-name range edges, multiline payload, mutual recursion,
composition + chain. Tackle each as a separate sub-item OR a
follow-up wave if they all share a root cause. If a single root
fix closes multiple, document.

### Item 11: verify

```
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
```

**Expected: pass rate > 95%** (some edge cases may still fail;
document precisely).

Also verify no regression:

```
./test.sh    # default, bootstrap path
```

Must stay green.

## Acceptance

1. Items 1-5 land with fixtures.
2. `--lang` under seam passes >= 155 / 164 claims (94%+).
3. Default `./test.sh` green.
4. No regression in earlier-wave fixtures.
5. Compiler.smt2 and sample.smt2 rebuilt + tracked.
6. Wave doc.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`, Python.
- "Make-it-green" hacks (e.g. hardcoding specific lang_test claim
  patterns).
- Skipping the lang re-verification (Item 11).

## Known gotchas

- `compiler/parse_body.ev` was extended in 4j (bare-`=`) and 4k
  (multi-name, ⇒, chained tail). 4l touched composition. Don't
  break those.
- Variant names are globally unique; constructor `Exit` etc.
  resolve at parse via the enum table.
- Sample.smt2 must be rebuilt (it embeds compiler.ev's translate
  rules). `scripts/build-sample-smt2.sh`.
- Per the wave-4m design, sample.smt2 holds many claims'
  constraints simultaneously — be careful introducing new shapes
  whose state grows non-trivially per claim.

## Reporting back

- Branch (`agent-47-equality-rhs-complex`).
- Items 1-5 status.
- Lang pass rate (headline).
- Test count delta (current: 107 kernel tests).
- Cite docs.

Be terse.
