# Task: fix sample.smt2 emit bugs (wave 4o)

## Why

Wave 4m landed `compiler/sample.ev → sample.smt2`, the lex-once
multi-claim sample driver. The implementation was complete enough
to run end-to-end but the verification phase never ran, and the
data from this session's full `--lang` pass under the seam revealed
TWO real bugs in the emit:

1. **Extra `;; claim:` markers for non-claim items.** For
   `tests/lang_tests/test_cons_chain_lit.ev` (1 real claim,
   1 enum), sample.smt2 emits **6** `;; claim:` markers:

   ```
   ;; claim: sat_user_intlist  ← the real claim
   ;; claim:                   ← 4 empty-name markers from stray tokens
   ;; claim:                   ←
   ;; claim:                   ←
   ;; claim:                   ←
   ;; claim: ICons             ← the enum's last variant name
   ```

   And 6 `(check-sat)` blocks. Bootstrap emits 1 verdict; we emit
   6. The wrapper zips markers with z3's sat/unsat output
   line-for-line, so the extra blocks produce garbage JSON.

2. **Race condition on `/tmp/compiler-input.ev`.** sample.ev's
   `ReadFile("/tmp/compiler-input.ev")` is baked into sample.smt2.
   When `sample-via-smt2.sh` runs in parallel (via parallelized
   `run-lang-tests.sh`), each invocation's `cp $FLAT
   /tmp/compiler-input.ev` races against every other's — one
   wrapper's input clobbers another's mid-kernel-run.

   *Quick fix already landed* (`mkdir` lock around the cp+kernel
   pair, commit `7ecc95e`). That makes the wrapper effectively
   serial — kills the parallelism win. This wave should remove
   the shared file path entirely so parallel works cleanly.

## Authorisation

Edit:
- `compiler/sample.ev` — fix the marker emission and the path.
- `compiler/compiler.ev` — same path-input fix applies; keep them
  in sync.
- `scripts/sample-via-smt2.sh` — remove the mkdir lock once the
  per-process path lands.
- `scripts/evident-self` — same.
- `tests/kernel/*.ev` — fixtures pinning the new behaviour.
- `docs/` — wave doc.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/wave-4m-lex-once-sample.md` — what 4m intended.
3. `docs/plans/blocked-sample-and-eq-fix.md` — the gate this
   closes.
4. `compiler/sample.ev` (the broken pieces).
5. `compiler/compiler.ev` (the working single-claim emit; sample
   shares structure).
6. `scripts/sample-via-smt2.sh` (the wrapper).
7. This session's `--lang` data (raw output captured at
   `/tmp/lang-seam.log`, debug probe at
   `/private/tmp/.../bbc4af95-.../tasks/b3swy6bgh.output`).

## Scope

### Item 1: don't emit `;; claim:` for non-claims

In wave 4m's design (its doc):

> `enum` items are NOT sampled (matching bootstrap's
> `schema_names`, which excludes enums); their datatypes go to
> the shared `_eacc` block.

Today: enums DO emit `;; claim: <last variant name>` blocks. Find
where `claim_block`/`skip_block` (lines 788-789 of sample.ev) are
gated. The gating predicate (`claim_done`/`skip_stop` at
lines 438/736) is currently triggering on non-claim items.

Fix: only emit a block when the item is actually a `claim` (or a
`fsm` / `schema` if those are sampled — verify against bootstrap's
`schema_names` for the lang corpus).

For enum items: route their datatype into `_eacc` (already
present) and emit NO marker, NO push, NO check-sat.

Verify with: byte-exact match between sample.smt2's marker count
and bootstrap's sample-output claim count on each lang file.

### Item 2: don't emit `;; claim:` with empty name

The 4 empty-name markers in the probe suggest a top-level item
parser is firing even when no real item is found (e.g. trailing
whitespace, comments, or a parse fail-through).

Fix: gate the block emit on `claim_name ≠ ""`. Or fix the
parser to not invoke the emit branch when no item is in flight.

### Item 3: remove the shared `/tmp/compiler-input.ev` race

Pick one of:

a. **Per-process path.** Change `ReadFile` to read from a path
   that includes the kernel's pid or a passed-in suffix. Requires
   either passing the path through stdin (read first line as the
   path) or via env (kernel reads env into the FSM). Adds a small
   protocol.
b. **Read source from stdin entirely.** Replace
   `ReadFile("/tmp/compiler-input.ev")` with a `ReadLine` loop
   that concatenates lines until EOF, then runs the existing
   processing. No external state at all. Cleanest but adds an
   EOF-detection lexer hop.

Whichever path is chosen, do BOTH `compiler.ev` and `sample.ev`
the same way so the wrapper logic stays consistent.

After this lands, `scripts/sample-via-smt2.sh` and
`scripts/evident-self`'s `emit_via_smt2_wrapper` can drop their
`cp $FLAT $INPUT_PATH` step (and the mkdir lock from commit
`7ecc95e`).

### Item 4: verify

After Items 1-3:

```bash
scripts/build-compiler-smt2.sh
scripts/build-sample-smt2.sh
# Single-file probe — byte-equal to bootstrap on the simplest file
$(EVIDENT_SELF_VIA_SMT2=1 scripts/evident-self bin) sample \
    tests/lang_tests/test_cons_chain_lit.ev --all --json
bootstrap/runtime/target/release/evident sample \
    tests/lang_tests/test_cons_chain_lit.ev --all --json
```

Both should output identical JSON `{"sat_user_intlist":true}` and
NOTHING ELSE.

Then run the full lang phase under the seam, parallel:

```bash
time EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
```

**Expected: green** AND wall-clock < 1.5h (parallelism works again
once Items 1-3 are done).

## Acceptance

1. Sample.smt2 emits exactly one `;; claim: <name>` per real claim
   (no enums, no empty names).
2. No `/tmp/compiler-input.ev` race — parallel sample-via-smt2.sh
   invocations don't clobber each other.
3. Single-file probe byte-equal to bootstrap.
4. Full `--lang` under seam green, < 1.5h wall.
5. `./test.sh` default green; no regression.
6. Fixture pinning the per-claim marker count.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`.
- Adding Python.
- Leaving the mkdir lock in `sample-via-smt2.sh` (the band-aid
  this wave replaces).

## Known gotchas

- The lex-once approach holds many claims' state simultaneously.
  Per-tick cost grows with state size — wave-4m's measurement
  showed sample.smt2 was the same wall-clock as per-claim (no
  amortization win in practice). That's a separate problem.
- Sample-via-smt2.sh strips `[functionizer]` lines. Don't break
  that.

## Reporting back

- Branch (`agent-46-fix-sample-emit`).
- Items 1-4 status.
- The lang phase headline (PASS rate + wall-clock).
- Test count delta (current: 107).
- Cite docs.

Be terse.
