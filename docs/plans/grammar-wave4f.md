# LEX bulk-skip — grammar wave 4f

Informed by, and citing throughout, the wave-4e perf diagnostic
(`docs/plans/grammar-wave4e-perf-diagnostic.md`).

## Why (the wave-4e finding)

Wave-4e measured the self-hosted compiler precisely and named the only
compiler-side lever:

- The functionizer extracts **0 / 37 123** steps; the whole monolithic
  `main` body is re-solved by Z3 every tick.
- Per-tick cost is **flat ~490 ms**, irreducible at the `compiler.ev`
  level, and independent of input (it is the static body size, not
  active state — wave-4e §3, §5).
- Therefore `total_wall ≈ per-tick × tick-count`, and **tick count is the
  only lever** (wave-4e §5, "Recommended refactor (wave 4f)").
- LEX was ~1 tick/char, and a real `.ev` file is mostly whitespace and
  `--` comments (wave-4e §5; `blocked-grammar-wave4d.md` Blocker 1).

Wave-4e's recommendation, verbatim in spirit: *"make the LEX FSM consume
runs of trivially-classified characters in a single tick"* — a whitespace
run collapsed to one tick, and a `--` comment consumed to end-of-line in
one tick. Hypothesised ~3× on a comment-heavy file.

## What changed

Two files carry the refactor; the three canonical driver fixtures and one
new fixture exercise it.

### `compiler/lexer.ev` — `WsRunLen` helper (new)

`claim WsRunLen(s, p, adv)` returns the length of the whitespace run
starting at `p`, **capped at 16**, via a bounded peek ladder
(`substr` + a 4-way whitespace test per offset). All locals are
`wr_`-prefixed to avoid the composition body-local leak
(`memory: project_claim_composition_leaks_body_locals`). `""`
(past-end-of-input) counts as whitespace so a trailing run walks cleanly
to EOF. `adv ≥ 1` always, so a lone separating space yields `adv = 1` —
**identical** to the old per-char `+1`.

### `compiler/compiler.ev` — `pos` now advances by a computed `next_pos`

The lex driver previously advanced `pos` by a hard `+1` per tick. It now
carries a state-computed `next_pos`:

```
next_pos = is_first_tick ? 0
         : lx_comment  ? lx_comment_to        -- jump to next '\n'
         : lx_skip_ws  ? pos + lx_ws_adv       -- jump past the ws run
         : pos + 1                             -- unchanged default
```

- **Whitespace run** (`is_ws ∧ in_range ∧ ¬_in_strlit`): advance by
  `WsRunLen` (≤16). A single space ⇒ `+1` ⇒ no behavioural change.
- **`--` line comment** (`cur='-' ∧ next='-'`, at top level, not mid-token,
  not in a string literal): `lx_comment_to = index_of(input,"\n",pos)` — or
  `#input` if no newline. Lands on the newline, which the next tick's
  ws-skip then consumes along with the following indentation.

The one subtlety that is easy to miss: the comment's first `-` is an
operator char, so the token-emission ladder's `char_only` arm would emit a
stray `Minus`. The `tokens` formula is therefore gated `lx_comment ?
_tokens : …` to suppress all emission on a comment tick. (Whitespace never
emits a token, so the ws path needs no such guard — that is also why the
ws-skip was byte-identical on the first try and the comment-skip was not.)

### Why the token stream is byte-identical

Whitespace and comments produce **no tokens**. Visiting fewer of their
positions cannot change the token stream — only the tick count. The
finish-trigger char (the first whitespace after an ident/int) is always
visited (advance lands exactly on it; we only skip *from* a whitespace
char), so token boundaries are never swallowed.

## Measurements

Bootstrap-built `compiler.smt2` before (wave-4e sources) and after
(wave-4f sources), same kernel, same inputs. Body grew 200 348 → 200 531
lines (**+183**, ~0.09%) — per-tick cost is unchanged; the win is purely
tick count.

Input A — comment-heavy, 409 chars (3 leading `--` lines, a `claim` with
two scalar memberships each preceded by a `--` comment, 4-space indents):

| compiler   | ticks | ticks/char | wall (kernel) |
| ---------- | ----- | ---------- | ------------- |
| wave-4e    | 531   | 1.30       | ~260 s (est, 531 × ~490 ms) |
| wave-4f    | **65**| 0.16       | **36 s** (measured) |

**8.2× fewer ticks; ~7× wall-clock.** Well past the 2× acceptance bar and
the wave-4e ~3× hypothesis — because both the comment bodies *and* the
inter-line indentation collapse.

Input B — same claim, comment-free, 47 chars (isolates ws-skip alone):
baseline **63** ticks → wave-4f **55** ticks (~1.13×). The whitespace-only
win is modest (short 4-space indents, mostly token chars); the comment
collapse is where the leverage is, exactly as wave-4e predicted.

Functionizer summary is unchanged in shape (still `0 JIT / 0 interp / all
residual` — wave-4f does not touch the extract ceiling, which wave-4e
showed is a separate kernel-side problem). The change is tick count only.

### `test_hello` (the task's smoke test)

`test_hello.ev` flattened = 4137 chars / 106 lines, **75% comment-line
chars** (confirming wave-4e's "~70% comments").

| compiler | ticks | wall |
| -------- | ----- | ---- |
| wave-4e baseline | ~5000+ (wave-4e §4 projection; ≈1.3 ticks/char ⇒ ≈5378) | ~40 min (wave-4e §4) |
| wave-4f | **1436** (measured) | **857 s ≈ 14.3 min** (measured) |

⇒ **~3.5× fewer ticks, ~2.8× wall-clock** — clears the 2× acceptance bar
and matches wave-4e's ~3× hypothesis.

Why test_hello (~2.8–3.5×) is below input A (8.2×): flattened test_hello
inlines the whole `stdlib/kernel.ev`, so a large fraction of its chars are
*code* (`Build*` claim bodies → many tokens). Bulk-skip cuts only the LEX
phase; the REVERSE phase (~1 tick/token) and PARSE phase are untouched, so
on a token-dense file they dilute the lex win. Input A — comment/whitespace
dominant with few tokens — is closer to the pure-lex ceiling. The win
scales with the comment+whitespace fraction: a comment-heavier file beats
test_hello, a token-dense file sees less. (Per-tick cost on test_hello is
higher than wave-4e's ~490 ms — ~600 ms, rising with the carried 4137-char
input string — but that scales both endpoints equally and does not affect
the ratio.)

## Correctness

- **Comment-skip**: self-host output of input A (comment-heavy) is
  byte-identical to the comment-free input B (`diff` empty). New fixture
  `tests/kernel/test_compiler_driver_canonical_comment.ev` pins this.
- **Whitespace-skip byte-identical**: the `canonical_match` (8-space
  indents) and `canonical_seq` (4-space indents) driver fixtures now run
  the bulk-skip FSM and still emit their exact expected SMT-LIB.
- Closes the wave-4d **Blocker 1** correctness gap: the lexer previously
  had no comment mode (`--` lexed as two `Minus` tokens).

## Scope / honesty

- `WsRunLen` is **bounded at 16**: a whitespace run longer than 16 takes
  `ceil(run/16)` ticks rather than one. Bounded (not `index_of`-based)
  because there is no "first non-whitespace" string primitive — only
  `index_of` (substring search), which fits comments (`\n` delimiter) but
  not the complement of a character set. 16 covers all realistic
  indentation; the ladder adds only ~33 assertions, negligible against the
  37 k body (so it does not raise per-tick cost — confirmed by the +183
  line delta).
- `index_of` and the per-tick `str.*` work are functionizer-hostile and
  stay residual on Z3 (as wave-4e noted for all String ops). That is fine:
  it is setup-time work and the lever was never per-tick cost.
- The order-of-magnitude win remains a kernel-side work-stack walker
  (wave-4e §5 "structural ceiling"; `blocked-grammar-wave4d.md` Blocker 5),
  out of scope for wave 4f.
