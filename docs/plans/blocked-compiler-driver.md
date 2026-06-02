# Blocked: compiler/compiler.ev file-reading path on multi-byte source

**Status (2026-06-02):** The MVP compiler driver lands and the
load-bearing integration test passes. One blocker prevents the
file-reading driver (`compiler/compiler.ev`) from processing *real*
`.ev` files (anything containing a Unicode operator like `∈`, `⇒`,
`⟨ ⟩`, `↦`, `≤`, …). The blocker is a frozen-kernel bug, not a
compiler-pass gap.

## What landed (works today)

- `compiler/compiler.ev` — the self-hosted driver: read source →
  lex (char-by-char) → parse → translate → emit `.smt2`. Composes
  `compiler/lexer.ev`, `compiler/parser.ev`,
  `compiler/translate_declare.ev`, `compiler/translate_bool.ev`,
  `compiler/translate_manifest.ev`. Nothing is hardcoded as tokens
  or AST.
- `tests/kernel/test_compiler_driver_mvp.ev` — the load-bearing
  integration test. Source string `"claim main\n    x ∈ Int = 5"`
  is **really lexed and parsed**; emits, byte-for-byte:

  ```
  ;; manifest: state-fields = x:Int
  ;; manifest: effects-name = effects
  ;; manifest: effect-enum-name = Effect
  ;; manifest: result-enum-name = Result
  ;; manifest: max-effects = 0
  (declare-fun x () Int)
  (assert (= x 5))
  ```

  This fixture passes because it carries its input as a **String
  constant** (recomputed from the literal each tick), which is
  immune to the bug below.

## The blocker: multi-byte String state-carry grows the string

`compiler/compiler.ev` reads its source with the consolidated-lexer
pattern (`ReadFile` on tick 0; `last_results[0]` →
`StringResult(s)`; then `input` carried across ticks via the
`_input` state pair). A multi-tick lexer **must** keep the whole
source string available every tick, so the source is necessarily
state-carried.

When the carried String contains a multi-byte UTF-8 codepoint, the
kernel's per-tick re-assert of `_input = <prev value>` **grows the
string on every tick**. Isolated repro (no compiler code involved):

```evident
-- read /tmp/mb.txt, carry it, print #input for 3 ticks
input  ∈ String
_input ∈ String
rr ∈ String = match last_results[0]
    StringResult(s) ⇒ s
    _ ⇒ "<none>"
input = (is_first_tick ? "" : (_input = "" ? rr : _input))
...
line ∈ String = "t=" ++ str_from_int(t) ++ " len=" ++ str_from_int(#input)
```

| file contents | `#input` per tick |
| ------------- | ----------------- |
| `aXb` (ASCII) | 3, 3, 3  ✅ stable |
| `a∈b` (∈ = 3 bytes) | 5, 8, 14  ❌ grows |

Because `pos` advances by 1 each tick while `#input` keeps growing,
`done = pos ≥ #input` is never reached: the FSM spins to the tick
limit instead of emitting. (Observed directly in `compiler.ev`:
`#input` traced as 28, 31, 37, 49, 73, 121, 217 over successive
ticks on the canonical `∈` source.)

The growth is in the **carry round-trip** through Z3's string
model, not in `substr`/`#` themselves: a String *constant*
containing `∈` lexes correctly (the integration test proves it).
Only re-asserting a previously-read multi-byte String corrupts it.

## Why this is not fixed here

The bug is in `kernel/` (the per-tick state-carry re-assert path)
and/or the Z3 string encoding the kernel uses — both **frozen**
(CLAUDE.md freeze table: `kernel/` edits require a written proposal
+ explicit user approval per edit). Per the freeze rules, the
correct action is to document the blocker and route around it, not
patch the kernel.

The driver therefore works **today** only on source with no
multi-byte operators. Since real Evident uses `∈`/`⇒`/`⟨⟩` heavily,
the file-reading path is effectively gated on this fix.

## Concrete next steps (pick one)

1. **Fix the kernel string state-carry (needs a `kernel/` proposal +
   user approval).** Make the post-solve String read-back and the
   next-tick `_<name> = <prev>` re-assert round-trip multi-byte
   UTF-8 losslessly (the likely culprit is byte- vs codepoint-length
   handling, or per-byte vs per-glyph escaping in the Z3 string
   construction — compare against the `translate::z3_string`
   escaping fix noted in the pretty-evident memory, which solved the
   sibling encode bug on the *emit* side). This is the right fix:
   it unblocks every multi-tick program that carries text, not just
   the compiler.

2. **Carry the source as a cons-list of single-char Strings**
   (`enum CharList = CLNil | CLCons(String, CharList)`), indexed per
   tick instead of `substr`-ing one big String. Whether the
   datatype-carry path dodges the same multi-byte round-trip bug is
   **unverified** — needs a spike. It is also a non-trivial lexer
   rewrite (the lexer would walk a char-list, not a string+pos).

3. **Single-tick lex (no carry).** Do the whole lex+parse+translate
   in the one tick after the read, so `input` is a tick-local
   derived from `last_results[0]` and never carried. Avoids the bug
   entirely but requires a bounded per-position unroll in one tick
   (the "mega-pipeline" shape prior notes flag as straining the
   translator on long inputs). Viable only for short fixed inputs.

Recommended: **option 1** — it is the root-cause fix and the only
one that scales to real source files.

## Acceptance status against the task

- [x] `compiler/compiler.ev` exists, is valid Evident, emits cleanly.
- [x] `tests/kernel/test_compiler_driver_mvp.ev` passes.
- [x] Emitted output matches the expected SMT-LIB byte-for-byte.
- [x] `./test.sh` green.
- [~] File-reading on multi-byte source — blocked by the kernel
      String state-carry bug above.
