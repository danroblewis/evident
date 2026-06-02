# Conformance migration wave 3 — disposition

Migrated the legacy Python conformance tests
(`tests/conformance/test_*.py`) into implementation-agnostic feature
specs under `tests/conformance/features/` (new dirs `017`–`138`),
then deleted every `test_*.py` and `__init__.py`. Only
`tests/conformance/conftest.py` remains (it goes in the final wave,
when `scripts/evident-self` replaces the bootstrap binary path).

## Migration mechanics

The feature runner compiles `source.ev` via `evident emit <claim>`
and runs the resulting `.smt2` through the kernel, checking
`smt2-contains` substrings + `exit`/`stdout`. The legacy tests used
`evident sample` (solve + inspect bindings / SAT-UNSAT). To bridge:

- **Value assertion** (`assert_binding(b, v, V)`): the source computes
  `v`, then `ok ∈ Bool = (v = V)` and `effects = ⟨Exit(ok ? 0 : 1)⟩`.
  Correct → exit 0, wrong value → exit 1, contradictory → exit 2.
  Works for any type (int incl. negative, string, real, enum, record
  field) and never surfaces a raw value (so it can't collide with a
  kernel halt code).
- **SAT-only** (`assert_sat`): `effects = ⟨Exit(0)⟩`, expect exit 0.
- **UNSAT** (`assert_unsat`): `effects = ⟨Exit(0)⟩`; a contradictory
  tick-0 solve makes the kernel exit 2.
- **`--given k=v`**: baked in as ordinary `=` constraints (identical
  constraint set).

### Two bootstrap-emit/kernel quirks worked around (NOT patched — frozen)

1. **`head` is reserved** in the kernel's generated SMT (sequence
   head) → "ambiguous constant reference". Renamed such vars to `pre`.
2. **Constant-folding drops manifest state-fields.** When a top-level
   primitive var appears *only* in pinned equalities (and especially
   when consumed by a claim-call inline), emit folds it to a literal
   and never declares the SMT constant — yet the manifest still lists
   it as a state-field, so the kernel aborts with "state var X not in
   model" (exit 3). Avoided by either (a) inlining literal call args,
   or (b) range-constraining a kept input (`6 < x < 8` instead of
   `x = 7`) so Z3 keeps the constant. Vars that appear in a kept
   inequality/composition survive folding, so most specs are
   unaffected.
3. **Claim composition only fires in the queried claim.** A bare /
   mapped / `..` / `cond ⇒ Claim` composition placed inside a `type T`
   body did **not** propagate to an instance `t ∈ T` under emit (the
   UNSAT cases came back SAT). The legacy tests queried the composing
   claim directly; the features therefore inline the composition into
   `main` (matching the original query target).

## Per-file disposition

| File | Disposition |
| ---- | ----------- |
| `test_language.py`          | migrated → 017–059 (minus the two below) |
| `test_errors.py`            | migrated → 060–066 (minus the four below) |
| `test_string_ops.py`        | migrated → 067–080 |
| `test_syntax_sugar.py`      | migrated → 081–093 (minus one below) |
| `test_claim_composition.py` | migrated → 094–124 |
| `test_composite_elements.py`| migrated → 125–138 |
| `test_cli.py`               | not migrated — CLI-surface (see below) |
| `test_evident_self.py`      | not migrated — CLI-surface |
| `test_selfhosted_diff.py`   | not migrated — script-harness |
| `test_selfhosted_perf.py`   | not migrated — script-harness |
| `__init__.py`               | deleted (no longer a package; nothing imports `.conftest`) |
| `conftest.py`               | KEPT (final wave) |

## Properties intentionally NOT migrated, with reason

These are **removed/unsupported syntax** (the self-hosted compiler is
not required to reproduce them) or **observables the feature format
cannot express**. None is a capability we need to preserve.

- `∀ v ∈ {1, 2, 3} : …` — forall over an *explicit set literal*. Under
  `emit` this is a hard "dropped constraint (couldn't translate to
  Bool)" error (it only works under `sample`). Forall over a *range*
  `{a..b}` translates fine and IS covered (feature 038, 123, 136).
  Drops: `test_forall_unsat`, `test_forall_forces_unsat`.
- `∃! v ∈ …` — exists-unique. `!` is a lex error; the syntax is
  unsupported. Drops: `test_exists_unique_unsat_no_match`,
  `test_exists_unique_unsat_multiple`.
- `s ⊑ "hello"` — the `⊑` prefix operator is a lex error (removed
  syntax). The prefix *capability* is covered by `starts_with(...)`
  (features 077/078). Drop: `test_string_starts_with_unsat`.
- `s ∈ /[a-z]+/` — regex literals are a removed feature (parse error).
  Drops: `test_regex_membership_unsat`, `test_regex_unsat`.
- `assert 'lonely' not in bindings` — asserts a binding is *absent*
  from the model (typo-defense for single-use arg inference). The
  emit+kernel format observes behaviour, not model variable presence,
  so a "this name is unbound" property is inexpressible. Drop:
  `test_arg_inference_skips_single_use`. (The positive half —
  multi-use names ARE inferred — is covered by 088/089.)

## The four CLI / script-harness files

`test_cli.py`, `test_evident_self.py`, `test_selfhosted_diff.py`,
`test_selfhosted_perf.py` assert the **bootstrap `evident` CLI surface**
(`sample`, `--json` binding shape, removed-subcommand rejection,
parse-error exit codes) and the **transition scripts**
(`scripts/evident-self`, `scripts/diff-test-selfhosted.sh`,
`scripts/bench-selfhosted.sh`).

These are not language capabilities and cannot live in the
implementation-agnostic feature corpus, which only knows
`source.ev → emit → kernel`. In the deletion-target world (kernel +
`compiler.smt2`, no bootstrap CLI, no transition scripts) the surfaces
they pin no longer exist — so they are "things we don't need," per the
wave-3 task rule. Two incidentally-language behaviours they touched are
already covered by the corpus as a side effect:

- **import resolution** — every feature `import "stdlib/kernel.ev"`.
- **multiple claims in one file** — most features define several
  claims (e.g. `add` + `main`, `IsPositive` + `main`).

Deleted with the rest; no feature added specifically for the CLI
surface.
