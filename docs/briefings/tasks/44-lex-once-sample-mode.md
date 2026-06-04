# Task: lex-once-multi-claim sample mode (wave 4m)

## Why

Wave 4j (`docs/plans/blocked-sample-and-eq-fix.md`) and wave 4l
should leave us with `sample`/`sample --all --json` working on the
self-hosted path AND every claim-body shape the lang corpus uses
covered. The ONLY remaining gate (Wall 1) is per-claim recompile
cost: each `kernel + compiler.smt2` run is ~90s; one `--lang`
pass = ~190 claims × ~60s ≈ **~5 hours wall-clock**. That makes
`./test.sh` unusable.

The fix is the wave-4i Option 1: a `sample.smt2` (or a mode of
`compiler.smt2`) that LEXES THE FILE ONCE and solves every claim
in one kernel run, amortising the parse over all claims. After
this lands, `--lang` should drop from hours to minutes — close to
bootstrap's wall-clock.

After 4m: ALL walls down. The cutover (wave 4n: test.sh rewrite,
drop bootstrap from evident-self, `rm -rf bootstrap/`) becomes
purely mechanical.

## Authorisation

Edit:
- `compiler/*.ev` — extend the compiler to handle the new mode,
  OR add a sibling `compiler/sample.ev` driver.
- `scripts/build-compiler-smt2.sh` / new
  `scripts/build-sample-smt2.sh` — build artifacts.
- `scripts/sample-via-smt2.sh` — switch to invoking the new
  lex-once mode rather than per-claim recompile.
- `scripts/evident-self` — only if dispatch must change.
- `tests/kernel/*.ev` — verification fixtures.
- `docs/` — wave doc.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — "What Evident is", "Composition mechanisms",
   the deletion path.
2. `docs/plans/blocked-bootstrap-cutover.md` — wave-4i's Option 1
   description (the design target for this wave).
3. `docs/plans/blocked-sample-and-eq-fix.md` — Wall 1 cost
   numbers, the architectural argument for amortisation.
4. `docs/plans/wave-4j-sample-and-eq-fix.md` — the per-claim
   wrapper (the thing this wave replaces).
5. `docs/plans/wave-4l-…md` — what 4l landed for the shapes.
6. `compiler/compiler.ev` — the existing single-claim emit driver.
7. `scripts/sample-via-smt2.sh` — the wrapper to update.
8. Bootstrap's `sample` impl
   (`bootstrap/runtime/src/main.rs::cmd_query_or_sample`) — for
   the wall-clock target.

## Scope

### Design choice (decide first; document in wave doc)

Two shapes work. Pick one based on which is cheaper to land:

**Option A: A new `sample.ev` driver compiled to `sample.smt2`.**
- Reads `.ev` source from `/tmp/compiler-input.ev` (like
  `compiler.ev` does today).
- Iterates the file's top-level claims, solving each.
- For each claim: builds the SMT-LIB representation in memory (a
  string), invokes a kernel built-in to run Z3 check-sat on it,
  collects sat/unsat verdicts.
- Emits JSON `{"name":bool,...}` to stdout.

Issue: requires a kernel built-in for "check-sat this SMT-LIB
fragment, return bool." The kernel doesn't have that. Adding it
is a small Rust change (allowed under the "kernel under active
construction" rule when the capability is needed).

**Option B: Extend `compiler.ev` with a `--sample-all` mode.**
- A new entry-mode in the existing driver.
- Reads source as today.
- For each claim, emit SMT-LIB AND a `(check-sat)` line in one
  big concatenated output.
- Output is a SINGLE multi-claim `.smt2` with `(push)` / `(pop)`
  around each claim's assertions, each followed by `(check-sat)`.
- A small wrapper script (`sample-via-smt2.sh` updated) runs
  this once through `z3` standalone (`z3` is at
  `/opt/anaconda3/bin/z3`, already used by 4j's wrapper).
- Parses z3's interleaved `sat`/`unsat` lines into JSON.

**Option B is preferred** — no kernel change, leverages existing
z3 standalone, less surface area. But verify it before committing
(some single-claim shapes may interact badly with `push`/`pop`
in z3's incremental mode).

### Item 1: implement (Option B path)

1. Add a multi-claim mode to `compiler.ev` (or a sibling driver
   `compiler/sample.ev`) that, given a `.ev` source, emits:
   ```
   <prelude — Result/last_results/Effect — emitted ONCE>
   <top-level decls — enum/type definitions — emitted ONCE>
   (push)
   <claim 1 assertions>
   (check-sat)
   (pop)
   (push)
   <claim 2 assertions>
   (check-sat)
   (pop)
   ...
   ```
2. Update `scripts/build-compiler-smt2.sh` (or add
   `build-sample-smt2.sh`) to produce a `sample.smt2`.
3. Verify the multi-claim emit on a simple 2-claim file matches
   "emit-each-separately-and-concat" output.

### Item 2: update sample-via-smt2.sh

Drop the per-claim loop. Invoke the new multi-claim path ONCE,
then pipe its output to `z3 -in` (or a temp file). Parse z3's
output:

```
sat
sat
unsat
sat
...
```

Map to `{"name1":true,"name2":true,"name3":false,"name4":true,...}`
using the claim-name order embedded in the multi-claim output as
SMT-LIB comments (`;; claim: <name>`).

### Item 3: verify on the simplest lang file

```
EVIDENT_SELF_VIA_SMT2=1 /tmp/wrap.sh sample \
  tests/lang_tests/test_enums_basic.ev --all --json > /tmp/probe.json
bootstrap/runtime/target/release/evident sample \
  tests/lang_tests/test_enums_basic.ev --all --json > /tmp/baseline.json
diff /tmp/probe.json /tmp/baseline.json
```

**Expected: empty diff AND wall-clock < 2 minutes** (the
amortisation target).

If diff non-empty: a remaining shape gap surfaced; document and
stop.

If wall-clock not improved: the architectural change isn't working
the way expected; document and stop.

### Item 4: full lang phase under the seam

```
time EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
```

Expected: green, < 30 minutes total. Document the wall-clock.

### Item 5: tighten the seam

If Items 1-4 succeed, drop the per-claim path from
`sample-via-smt2.sh` entirely — there's no benefit to keeping it.

## Acceptance

1. Multi-claim mode lands (compiler or sibling driver).
2. `sample-via-smt2.sh` uses lex-once amortisation.
3. `test_enums_basic.ev` probe: byte-equal to bootstrap, < 2 min.
4. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` green, < 30
   min wall-clock.
5. `./test.sh` (default) green; no regression.
6. Test count delta documented.

## Forbidden

- Editing `bootstrap/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`, Python.
- Editing `kernel/` UNLESS Option A is the chosen path AND
  Option B was tried and proven infeasible.
- Skipping Items 3 and 4.

## Known gotchas

- z3's `(push)`/`(pop)` resets assertions to the surrounding
  scope; if any common decls are inside the pushed block they get
  popped too. Put shared decls (Result, Effect, enums) BEFORE the
  first push.
- The functionizer diagnostic `[functionizer] ...` is on stdout
  from the kernel; strip with `grep -v '^\[functionizer\]'`.
- A single multi-claim run emits LOTS of output (one .smt2 with
  every claim's assertions); use temp files, not in-memory pipes,
  for diagnosability.
- The kernel may have a per-tick limit (100,000 per CLAUDE.md);
  a multi-claim emit might take many ticks. Verify it doesn't
  hit the limit on the largest lang file.

## Reporting back

- Branch (`agent-44-lex-once-sample-mode`).
- Design choice (A or B).
- Items 1-5 status.
- Lang probe wall-clock (the headline).
- Cite docs.

Be terse.
