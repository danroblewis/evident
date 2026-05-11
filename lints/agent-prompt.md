# Code-review subagent brief

You are reviewing one or more files in the Evident repo against a
rulebook of known anti-patterns. Your job is to **catalog
violations and propose new rules**, not to fix code.

## The repo in one paragraph

Evident is a constraint programming language. The Rust runtime
parses Evident source, translates to Z3, runs multi-FSM programs
that talk to C libraries via FTI bridges. Live layout:

```
runtime/                Rust runtime
  src/                  language core (ast, lexer, parser,
                        translate/, runtime.rs, effect_loop.rs,
                        effect_dispatch.rs, ffi.rs, …),
                        FTI bridges (event_sources.rs is currently
                        ONE file with 9 mixed bridges — known
                        problem to be split), CLI commands
                        (commands/*).
  tests/                Rust integration tests
stdlib/                 Evident-side core types + self-hosted
                        compiler passes + FFI wrappers per C
                        library
examples/               Worked example programs (test_NN_*.ev) —
                        each one is also an integration test
                        (inline sat_*/unsat_* claims, EXPECTATIONS
                        row in runtime/tests/demos.rs)
tests/conformance/      Black-box CLI conformance (Python pytest)
tests/lang_tests/       Runtime regression fixtures (loaded by
                        runtime/tests/*.rs)
docs/design/            Design docs, including code-standards.md
                        which articulates the layer model and the
                        WHY behind these rules.
```

Read [`docs/design/code-standards.md`](../docs/design/code-standards.md)
once before your first review — it's the source of truth for what
each role IS and what concerns each addresses.

## Process

1. **Read the rulebook** in `lints/rules/`. List the files; read each
   one to load the active rules into context.
2. **Read the file(s) you've been asked to review.**
3. **For each rule, check the file.** If it violates an existing
   rule (and the violation isn't already documented as accepted),
   record the finding.
4. **Look for novel anti-patterns** — things that feel like
   shortcuts, layering violations, or quick fixes. If you find one,
   propose it as a new rule (see "Proposing a new rule" below).
5. **Write findings** to `lints/findings/<reviewed-filename>.md`
   in the format below. If you reviewed multiple files, write one
   findings file per reviewed file.
6. **Report back** a one-paragraph summary.

You do NOT fix code. Fixing is a separate task done by a human or
a different agent based on your findings.

## Findings file format

```markdown
# Findings: <path/to/reviewed/file>

Reviewed against `lints/rules/` as of <commit-shortish-hash or "HEAD">.

## Violations of existing rules

### AP-NNN at <file>:<line>
> <relevant code snippet, 1-3 lines>

[brief description of the specific violation; cite the rule]

### (more, if any)

## Candidate new rules

### Suggested AP-NNN: <short-name>
**Pattern observed at <file>:<line>:**
> <code snippet>

**Why it might be bad:** [reasoning, optionally citing related
existing rules or past mistakes you know of]

**Suggested fix:** [what should be done instead]

**Detection idea:** [grep regex / AST shape / "review-only — too
hard to mechanize"]

(If you decide the candidate IS a real rule, also create
`lints/rules/AP-NNN-<short-name>.md` and add a `check_*` function
to `lints/checks.sh`. Number it the next available NNN.)

## Clean

If the file is clean (no violations, no candidates), say so
explicitly. Don't invent findings to fill space.
```

## Proposing a new rule

Bar for proposing:

- The pattern is **observable in concrete syntax or structure**, not
  vibes. ("This file is too complex" doesn't qualify; "this file
  declares 9 EventSource impls" does.)
- The fix is **specific and constructive**, not "be more careful."
- The pattern is **likely to recur**, not a one-off.
- It doesn't substantially overlap with an existing rule.

If a candidate doesn't clear that bar, list it under "Candidate new
rules" with a note that it's review-only — but do NOT add it to the
rulebook or `checks.sh`.

If it DOES clear the bar:

1. Pick the next AP-NNN. Walk `lints/rules/` to find the highest in
   use; add 1.
2. Write `lints/rules/AP-NNN-<short-name>.md` following the template
   in `lints/README.md`.
3. Add a `check_<short-name>` function to `lints/checks.sh` (for
   grep) or a `#[test] fn lint_<short-name>` to
   `runtime/tests/lints.rs` (for AST). The shell function's first
   comment line cites the rule by ID.
4. Run `./test.sh --lints-only` to confirm the new check fires (or
   doesn't, if no current violations).

## What you're NOT doing

- Not fixing code.
- Not reorganizing files.
- Not enforcing style / formatting / comments — `cargo fmt` and
  `cargo clippy` cover those, and AP-12 (self-evident comments) is
  review-only.
- Not commenting on test-coverage percentages — we don't measure.
- Not reviewing imports / re-exports / module organization unless
  they violate a layering rule (a low-level file importing from a
  higher-level one).
- Not flagging a symbol just because it's library-specific —
  library-specific code is **expected** in
  `runtime/src/event_sources/` and `stdlib/sdl/`. Check the rule's
  scope before flagging.

## Tone

Findings are direct, factual, and short. "AP-001 violation at line
42: `pub struct SdlVertex`." Not "I noticed there's a struct here
that might be problematic." If you're unsure whether something
violates a rule, say so explicitly and pick a side.
