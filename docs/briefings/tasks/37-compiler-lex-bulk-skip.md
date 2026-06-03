# Task: LEX bulk-skip (wave 4f) — cut tick count via run consumption

## Why

The wave 4e perf diagnostic
(`docs/plans/grammar-wave4e-perf-diagnostic.md`) named the
bottleneck precisely:

- Functionizer extracts 0/37,123 steps (the compiler.ev body
  hits a 27.5% JIT-eligibility ceiling).
- Per-tick cost is a flat ~490 ms, irreducible at the
  compiler.ev level.
- Total wall = per-tick × tick-count. **Tick count is the only
  lever.**
- LEX is ~1 tick/char. `test_hello` is ~70% whitespace/comments.

Recommended refactor: make the LEX FSM consume **runs** of
trivially-classified characters in one tick. Hypothesized speedup:
~3× on a real comment-heavy file.

This is wave 4f — the refactor informed by wave 4e's diagnostic.

## Authorisation

Edit `compiler/lexer.ev` + `compiler/compiler.ev`'s lex driver
and test fixtures. No `bootstrap/`, no `kernel/`, no `stdlib/`,
no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-wave4e-perf-diagnostic.md` — the
   diagnostic. **READ ITS RECOMMENDATION** section carefully — it
   names the exact shapes to consume in runs.
3. `compiler/lexer.ev` — the FSM you're modifying.
4. `compiler/compiler.ev` — the lex driver.
5. `tests/kernel/test_compiler_driver_canonical_*.ev` — make sure
   they still pass byte-identical.

Cite #2 in your report.

## Scope

### Item 1: bulk-skip whitespace

Today the LEX FSM advances `pos` by 1 per tick. When the current
character is whitespace (space, tab, newline), advance to the
NEXT non-whitespace character in the same tick (using
`indexof` or a similar scan operation).

### Item 2: bulk-skip line comments

When the current character is `-` AND the next is `-` (line
comment start), advance to the next newline character (or
end-of-input) in the same tick. This is the biggest single win
since `test_hello` is mostly comments.

### Item 3: keep per-token logic per-tick

Identifiers, integer literals, string literals, operator tokens —
these stay one tick per token (or whatever they were). The
optimization is ONLY for whitespace runs and comment skipping,
where each char today consumes a tick but produces no useful
output.

### Item 4: measure

After implementing items 1-3:

- Re-time the smoke test on `test_hello`. Document new
  wall-clock.
- Run `EVIDENT_FUNCTIONIZE_STATS=1` and capture the new
  tick-count + per-tick mean.
- Compare to the wave-4e baseline.

If speedup is meaningfully less than 3×, document why.

## Acceptance

1. LEX FSM bulk-consumes whitespace runs and `--` comments to
   end-of-line.
2. All existing fixtures still pass byte-identical (the emitted
   SMT-LIB MUST NOT change — token stream is identical, just
   produced in fewer ticks).
3. Measured speedup on `test_hello`: at least 2× wall-clock.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. Diff scoped to `compiler/lexer.ev` + possibly
   `compiler/compiler.ev` (lex driver) + new
   `docs/plans/grammar-wave4f.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Changing the EMITTED tokens — only the path to producing them.
- Tackling other functionizer-hostile shapes; that's a separate
  conversation.

## Known gotchas

- Op/Token/Expr variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- `indexof("\n", pos)` and similar scan operations should
  work — Z3 supports them — but verify the kernel handles them
  if they show up in state-carry (the diagnostic noted String
  ops are functionizer-hostile, so they'll be residual, but
  that's fine for setup-time work).

## Reporting back

- Branch pushed.
- Before/after wall-clock on `test_hello` smoke test.
- Functionizer summary delta (tick count especially).
- `./test.sh` final line.
- Cite docs.

Be terse.
