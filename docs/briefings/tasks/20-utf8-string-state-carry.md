# Task: Fix multi-byte UTF-8 String state-carry kernel bug

## Authorisation

User authorised kernel work as part of the deletion-path plan:

> *"Plan to do those in order."* — pointing to (1) UTF-8 fix, (2)
> Cons→Seq sweep, (3) FTI honesty, (4) conftest.py + test.sh.

You may edit `kernel/src/tick.rs` and any supporting kernel files.
Same authorisation envelope as tasks #11/12/18/19.

## Why

`docs/plans/blocked-compiler-driver.md` documents this bug. Briefly:
when a String containing a multi-byte UTF-8 codepoint
(`∈`, `⇒`, `⟨`, `⟩`, `↦`, `≤`, …) is carried across ticks via
the `_<name>` state pair, the kernel's per-tick re-assert of
`_input = <prev value>` **grows the string each tick**.

Observed repro (from the blocker doc):

| file contents | `#input` per tick |
| ------------- | ----------------- |
| `aXb` (ASCII) | 3, 3, 3  ✅ stable |
| `a∈b` (∈ = 3 bytes) | 5, 8, 14  ❌ grows |

This blocks `compiler/compiler.ev`'s `ReadFile` path on every real
`.ev` file (they all use Unicode operators). The MVP test passes
only because it carries the source as a String constant
(recomputed from the literal each tick, immune to the bug).

The bug is in the state-carry round-trip through Z3's string
model, NOT in `substr`/`#`/`++` themselves: a String **constant**
containing `∈` lexes correctly.

## Required reading

1. `CLAUDE.md` — note the kernel row is "active construction."
2. `docs/plans/blocked-compiler-driver.md` — the full bug write-up
   with the isolation repro.
3. `kernel/src/tick.rs` — the pin-application path (the `apply_pins_a`
   function from task #12) is where state-carry equalities get
   asserted.
4. `docs/plans/architecture-invariants.md` — Z3 lifecycle rules
   (don't break them; the fix should not require simplification
   inside the tick loop, etc.).
5. `compiler/compiler.ev` — the affected consumer; once your fix
   lands, you should verify `compiler/compiler.ev` can use its
   ReadFile path.

Cite #2 and #3 in your report.

## Investigation hints

The bug is likely one of:

- **The kernel reads the model's String value as bytes, then
  re-asserts it as if the bytes were UTF-8-encoded characters.** Z3
  may be storing/returning the String as a UTF-8 byte sequence
  (the encoding `translate::z3_string` uses per project memory
  entry "Pretty self-host result") but the re-assertion path
  doesn't round-trip it correctly — it re-encodes already-encoded
  bytes.
- **Or:** Z3's `Z3_mk_string` and `Z3_get_string` use different
  encodings (e.g. Z3 stores `\u{2208}` for `∈` internally but the
  kernel reads back UTF-8 and then asserts the UTF-8 bytes as if
  they were the original characters, doubling them).
- **Or:** the per-tick re-assert uses textual SMT-LIB syntax (e.g.
  `(assert (= _input "a∈b"))`) and Z3's SMT-LIB parser treats the
  UTF-8 bytes literally — so each round-trip through the textual
  parse expands them.

Pinpoint the actual mechanism. The isolation repro in the blocker
doc is small; run it under the kernel with some `eprintln!`
instrumentation to see exactly what's being asserted each tick.

## What you're producing

1. **Identify the exact mechanism** of the growth. Document in one
   paragraph in your report.

2. **Fix in `kernel/src/tick.rs`** (or wherever the bug lives — it
   may be in a string-handling helper). The fix must:
   - Preserve invariant #1 (model parsed once).
   - Preserve invariant #2 (pins are equalities, not body changes).
   - Preserve invariant #4 (no per-tick `.simplify()`).
   - NOT regress the existing 70 kernel tests.

3. **Regression fixture** at `tests/kernel/test_utf8_state_carry.ev`
   that:
   - Reads a small file with multi-byte UTF-8 contents (e.g.
     `/tmp/utf8-test.txt` containing `a∈b`).
   - State-carries the contents across at least 5 ticks.
   - Asserts `#input` stays constant (3 in the `a∈b` case, since
     it should be 3 characters or 5 bytes — pick whichever Evident
     `#string` actually means and document it).
   - Uses `-- expect:` to make the test self-verifying.

4. **End-to-end demo** — modify `tests/kernel/test_compiler_driver_mvp.ev`
   OR add a new `tests/kernel/test_compiler_driver_readfile.ev`
   that reads `claim main\n    x ∈ Int = 5` from a file via
   `ReadFile` and emits the same SMT-LIB as the constant-input
   version. This proves the fix unblocks the real driver path.

## Acceptance

1. `kernel/src/tick.rs` (or wherever) modified with the minimal fix.
2. `tests/kernel/test_utf8_state_carry.ev` passes.
3. `tests/kernel/test_compiler_driver_readfile.ev` passes — same
   SMT-LIB output as the constant-input version.
4. `./test.sh` is fully green in all 3 modes (default, `EVIDENT_FUNCTIONIZE_JIT=0`,
   `EVIDENT_FUNCTIONIZE=0`).
5. The `docs/plans/blocked-compiler-driver.md` is updated with a
   "FIXED" header at the top citing the commit hash.
6. Diff scoped:
   - `kernel/src/tick.rs` (+ maybe a sibling helper file)
   - `tests/kernel/test_utf8_state_carry.ev` (new)
   - `tests/kernel/test_compiler_driver_readfile.ev` (new) OR
     modification to the existing MVP test
   - `docs/plans/blocked-compiler-driver.md` (FIXED marker)

## Forbidden

- Editing `bootstrap/`, `compiler/` (you should be able to verify
  the fix without modifying compiler.ev itself; if you must, ask
  via blocked-*.md), `stdlib/`, anything outside the named paths.
- Adding Python.
- Working around the bug at the caller-side (forcing every
  String-state-carry consumer to do byte-counting) — fix the bug
  at the kernel layer.
- Calling `.simplify()` inside the tick loop.
- Removing or weakening any existing invariant.

## Reporting back

- Branch pushed.
- One paragraph: what the actual mechanism was.
- Before/after lengths of `#input` for the `a∈b` fixture across 5
  ticks.
- `./test.sh` final line.
- Confirm `tests/kernel/test_compiler_driver_readfile.ev` produces
  the same SMT-LIB as the constant-input MVP test.
- Cite docs.

If the actual mechanism turns out to be something Z3-side that
can't be fixed cleanly in `tick.rs` (e.g. Z3 itself stores strings
in a way that fundamentally can't round-trip), write
`docs/plans/blocked-utf8-deeper.md` documenting it and suggest the
next move (could be: switch the kernel's String representation to
something other than Z3 strings, or pre-encode source files into
an ASCII-safe form before reading).
