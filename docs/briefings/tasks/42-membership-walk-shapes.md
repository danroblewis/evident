# Task: extend membership walk — 5 shapes (wave 4k)

## Why

Wave 4j (`docs/plans/blocked-sample-and-eq-fix.md`) landed `sample`
and the bare-`=` fix but Item 5 (lang green) blocked on Wall 2: the
lang corpus uses 5 claim-body shapes the self-hosted compiler
silently drops. Each dropped constraint flips a sat/unsat verdict,
making lang_tests structurally impossible to pass without these.

This wave closes Wall 2 — the membership-walk extensions. Wall 1
(per-claim recompile cost ~5h/lang-pass) is a separate, later wave
(a lex-once-multi-claim sample.smt2 mode, per wave-4i Option 1).

## Authorisation

Edit:
- `compiler/parse_body*.ev`, `compiler/translate*.ev` — the
  membership walk and its translators.
- `compiler/compiler.ev` — only if a driver-level change is needed
  to thread new tokens.
- `tests/kernel/*.ev` — one fixture per shape.
- `docs/` — wave doc.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — esp. "Chained membership", "Composition mechanisms",
   "Boolean and precedence footguns".
2. `docs/plans/blocked-sample-and-eq-fix.md` — the 5-shape table at
   the top of Wall 2.
3. `docs/plans/wave-4j-sample-and-eq-fix.md` — what wave 4j actually
   touched (parse_body.ev `ms_is_bare`).
4. `compiler/parse_body.ev` — current membership-step handler;
   start here.
5. `compiler/parse_body_ctor.ev` / `compiler/parse_body_seq.ev` /
   `compiler/parse_body_match.ev` — sibling handlers.
6. `tests/lang_tests/test_enums_basic.ev` and
   `test_chained_membership.ev` — the precise source shapes the
   compiler must translate.
7. Bootstrap's parser for the same shapes:
   `bootstrap/runtime/src/parser/` — port equivalents, do NOT edit
   bootstrap.

## Scope — one item per shape

Each item: parser+translator extension + one
`tests/kernel/test_compiler_driver_<shape>.ev` fixture proven
byte-identical to bootstrap (or semantic if byte differences are
benign). Land each item independently if you can; if intermediate
items break others, document the order.

### Item 1: multi-name decl `a, b, c ∈ T`

Per CLAUDE.md "Chained membership": `a, b, c ∈ Int < 5` is three
decls each bounded.

Today: `a, b ∈ Day` parses `t1 = Comma`, not `∈`/`=`. The walk
mis-emits `(declare-fun a () b)` and stops. Lang fixture site:
`test_enums_basic.ev:98`.

Extend the membership walk to consume Comma-separated names BEFORE
the operator (`∈` or `=`). Translation: emit one `(declare-fun
<name> () <Type>)` per name. Bound (`< N` etc.) applies to each
identically.

### Item 2: implication `⇒` body lines

29 occurrences in lang corpus. Example:
`today = Sat ⇒ is_weekend = true` (test_enums_basic.ev).

Today: bare-`=` handler in Item-1-of-4j consumes `today = Sat`,
leaves `⇒ …` dangling. The walk treats the consequent as a new
line, eventually mis-parses, drops the rest.

Fix: a body-line starting with `LHS ⇒ RHS` (where LHS and RHS are
each a member-step expression) emits `(assert (=> <lhs> <rhs>))`
rather than the bare assertion.

Per CLAUDE.md "Boolean and precedence footguns": `⇒` binds tighter
than `∧`. Wrap compound consequents — the parser must honor this.

### Item 3: chained bound `name ∈ Int < N` (range form)

`pos_x ∈ Int < 100` and `0 < x ∈ Int < 10` per CLAUDE.md
"Chained membership". Lang site: `test_chained_membership.ev:27`.

Today: membership walk consumes `∈ Int` and stops; the `< 100`
suffix is dropped → constraint lost.

Fix: after the type token, scan for trailing comparison operators
(`< N`, `<= N`, `> N`, `>= N`) and emit additional `(assert (< name
N))` etc. Range form `lo < name ∈ T < hi` requires recognizing the
prefix `lo <` BEFORE the name; handle as a 2-pass scan or extend
the head parser.

### Item 4: chained `≠` in decl `name ∈ Int ≠ 0`

Similar to Item 3 but for inequality. Today not handled.

Translation: `(assert (not (= name 0)))` after the decl.

### Item 5: claim composition (bare `ClaimName` line)

Per CLAUDE.md "Composition mechanisms": a bare `ClaimName` body
line means "inline this claim's body via names-match." Sites:
`is_weekend_rule`, `bounded_score`, `is_ok_value` in lang corpus.

Today: a bare Ident with no `∈`/`=` is mis-read or the walk stops.

Fix: an Ident-only body line with NO follow-on token (newline next)
is a composition: look up `ClaimName` in the prog's claim table,
inline its body's translated constraints into the current claim's
output. Names match by default — the composed claim's body-local
names refer to the SAME variables as the parent's.

**Subtle**: per
[[project-claim-composition-leaks-body-locals]], the callee's
body-local names UNIFY with the caller's. The compiler today
handles this via prefix; verify the prefixing is wired for the
bare-Ident path or extend it.

### Verification

Each item:
- Fixture in `tests/kernel/test_compiler_driver_<shape>.ev`.
- `scripts/diff-vs-bootstrap.sh --semantic <fixture> <claim>` →
  exit 0.
- 102 (or N+5) kernel tests, 0 failed.
- Default `./test.sh` green.

After all 5: probe a single lang_test file end-to-end via the
seam:

```
EVIDENT_SELF_VIA_SMT2=1 scripts/evident-self bin > /tmp/wrap.sh
chmod +x /tmp/wrap.sh
/tmp/wrap.sh sample tests/lang_tests/test_enums_basic.ev --all --json > /tmp/probe.json
bootstrap/runtime/target/release/evident sample tests/lang_tests/test_enums_basic.ev --all --json > /tmp/baseline.json
diff /tmp/probe.json /tmp/baseline.json
```

`diff` empty → the shape coverage is complete for this file.

If the probe times out (compile cost ~90 s × ~19 claims ≈ 30 min
on this file alone), capture the partial result and document. Wall
1 is acknowledged out-of-scope; don't try to address it here.

## Acceptance

1. All 5 shape extensions land with fixtures.
2. Each fixture passes `diff-vs-bootstrap.sh --semantic`.
3. `./test.sh` green default mode + under `FUNCTIONIZE=0`
   (kernel phase).
4. The lang-test single-file probe (test_enums_basic.ev) produces
   bootstrap-matching JSON OR documents the precise next gap.
5. No regression on wave 4g/4h/4j fixtures.
6. Diff scoped to `compiler/*.ev` + new fixtures + wave doc.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`.
- Adding Python.
- Implementing the lex-once-multi-claim mode (Wall 1; that's a
  later wave).
- "Make-it-green" hacks like hardcoding lang_test claim names.

## Known gotchas

- Op/Token names are globally unique. New tokens (if any) must not
  collide.
- Composition leaks callee body-local names — prefix as needed.
- `⇒` binds tighter than `∧` per CLAUDE.md; respect this.
- The Item 4j-1 bare-`=` fix only handled `name = atom` after a
  prior `name ∈ T`. Item 2's `LHS ⇒ RHS` needs to handle LHS with
  any operator (`=`, `<`, `∈`, etc).
- Each compile via `compiler.smt2` is ~minute(s). Don't iterate
  the smoke-probe more than necessary.

## Reporting back

- Branch (`agent-42-membership-walk-shapes`).
- Items 1-5 status.
- The lang single-file probe result (the headline).
- Test count delta (current: 102).
- Cite docs.

Be terse.
