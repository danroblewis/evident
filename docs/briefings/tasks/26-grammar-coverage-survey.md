# Task: Grammar coverage survey

## Why

`compiler/compiler.ev` (the self-hosted compiler) is being
extended in waves. Wave 1 (session A, in flight in parallel)
adds multi-membership + arithmetic expression pins. Subsequent
waves need to know what's still missing to reach the goal —
`compiler.ev` compiling itself + the test corpus.

This task is a **read-only survey**. Produce one document. No
code changes.

## Authorisation

Read everything. Write only one new doc at
`docs/plans/grammar-coverage-survey.md`. No edits to anything
else.

## What you're producing

`docs/plans/grammar-coverage-survey.md` containing:

### Section 1: Universe of grammar shapes used

Survey:
- `compiler/*.ev` (the self-hosted compiler's own source)
- `stdlib/*.ev` (library code the compiler depends on)
- `tests/kernel/*.ev` (the integration test corpus)
- `tests/lang_tests/*.ev` (the language-spec corpus)

For each Evident grammar shape used, count occurrences.
Categories to track (refine as you go):

- Claim/type/schema/fsm declarations
- Membership `x ∈ T` with/without `= rhs`
- Expression atoms: `EInt`, `EIdent`
- Binary ops: `+ - * / = ≠ < ≤ ∧ ∨ ⇒`
- Ternary `? :`
- `match` expressions + patterns + guards
- `enum` declarations + variants (with/without payloads)
- `Seq(T)` literals + ops (`++`, `#`, `seq[i]`)
- `String` literals + ops (`#s`, `substr`, etc.)
- Quantifiers `∀ x ∈ S : …`, `∃`
- Composition mechanisms (per CLAUDE.md §"Composition mechanisms"):
  `..ClaimName`, `ClaimName`, `ClaimName(slot ↦ value)`,
  `(a, b) ∈ ClaimName`, `cond ⇒ ClaimName`,
  `recv.subclaim(args)`, `subclaim Name`
- Imports
- Generics `<T>`
- Subclaims (nested)
- `is_first_tick` / `_<name>` state-carry conventions
- Effects + `LibCall` + `Result` matching
- FTI use sites

### Section 2: Coverage map

What does `compiler/compiler.ev` handle TODAY (the MVP):
- ASCII literal pins on a single Int membership.

What does wave 1 add (session A is landing):
- Multi-membership; arithmetic pins.

For each remaining shape in section 1, mark:
- **WAVE 2 candidate** — small extension, immediate next step.
- **WAVE 3+** — bigger lift, can wait.
- **Self-hosting blocker** — `compiler.ev` itself uses this; can't
  self-compile without it.

### Section 3: Recommended wave plan

Propose waves 2, 3, … with concrete grammar lists for each.
Optimise for:
- Minimum to enable self-compilation (compiler.ev → compiler.smt2).
- Order that respects dependencies (no wave can require an
  earlier wave's deliverable).

### Section 4: Open questions

Anything you couldn't resolve from reading the code alone:
ambiguous grammar shapes, things compiler/parser.ev seems to
support but the translator doesn't, etc. List them so future
sessions can decide.

## Acceptance

1. `docs/plans/grammar-coverage-survey.md` exists and has the 4
   sections.
2. Section 2 lists every category from section 1.
3. Section 3 has at least waves 2-4 named with concrete shape
   lists.
4. `./test.sh` unchanged (you didn't touch anything else).
5. Diff scoped to that one new file.

## Forbidden

- Editing anything except the new survey doc.
- Adding Python.
- Modifying the test corpus, stdlib, compiler, bootstrap,
  kernel.
- Speculating about what would be "nice to have" — stick to
  what's USED.

## Reporting back

- Branch pushed.
- Top-line: how many distinct shapes used, how many waves
  proposed.
- Path to the survey doc.
- Cite the corpora you scanned.

Be terse. The coordinator reads the doc directly.
