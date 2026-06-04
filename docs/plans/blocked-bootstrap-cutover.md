# BLOCKED: bootstrap cutover (Spec 40)

**Status: BLOCKED at Item 1 (probe). Did NOT proceed to test.sh rewrite
or `rm -rf bootstrap/`.**

The cutover is gated on all three probe phases (`--kernel`,
`--conformance`, `--lang`) passing under the seam
(`EVIDENT_SELF_VIA_SMT2=1`). The **lang phase fails hard and cannot be
made green without a new self-hosted capability**. Details below.

Pre-flight (Item 0) succeeded: `scripts/build-compiler-smt2.sh` rebuilt
`compiler.smt2` (10,946,211 bytes / 226,984 lines, matching wave 4h),
and `scripts/check-deletable.sh` showed exactly the two expected
pre-cutover blockers (`test.sh` references bootstrap; `bootstrap/`
exists).

---

## Blocker 1 (DECISIVE): the self-hosted toolchain has no `sample`

### Symptom

```
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
── Phase 4: lang_tests (tests/lang_tests/) ──
11 files, 0 claims, 11 failed
  FAIL test_enums_basic.ev::load: evident-self(smt2): only 'emit' is supported, got 'sample'
  ... (all 11 files, identical error)
```

`scripts/run-lang-tests.sh` drives every `tests/lang_tests/*.ev` through
`evident sample <file> --all --json` and asserts that `sat_*` claims are
SAT and `unsat_*` claims are UNSAT. There are 11 files; `test_enums_basic.ev`
alone declares 19 such claims. This is the single source of truth for
*language* correctness (conformance tests the CLI; lang tests test the
language semantics — per the headers of both runners).

### Root cause — not a harness fixup, an architectural gap

`sample --all` is a **one-shot, multi-claim satisfiability check**: solve
each named claim's constraints independently, report sat/unsat per claim,
no I/O, no FSM. This is a distinct CLI verb from `emit` (compile a single
target claim to a runnable FSM `.smt2`).

The self-hosted system is exactly **kernel + compiler.smt2**, and neither
provides `sample`:

1. **The kernel has one mode: run a `.smt2` FSM.** `kernel --help` is read
   as a filename (`read --help: No such file or directory`). There is no
   sat-check verb.

2. **The self-hosted compiler emits a single target claim and has no
   claim-selection argument.** From `compiler/compiler.ev:291-296`
   (verbatim):

   > we have no claim-name argument (the source is read from disk), so we
   > use the corpus convention: the entry-point claim has NO first-line
   > params … a parametrized claim is SKIPPED outright … when multiple
   > bare-head claims appear the LAST one wins — the file's target claim.

   So even an "emit each claim, run on kernel, map exit code → sat/unsat"
   rewrite of `run-lang-tests.sh` is **infeasible**: the compiler cannot
   select `sat_bare_declaration` vs `unsat_two_different_variants` — it
   only ever emits the *last* bare-head claim of the file. A lang file has
   ~19 bare-head claims; 18 of them are unreachable through the
   self-hosted path. (And mapping kernel exit codes to sat/unsat is itself
   an unproven proxy — the kernel runs an emitted program as an FSM with
   effects/halting semantics, not as a constraint check.)

### Why this is signal, not something to route around

The task's forbidden list bars editing `tests/` to paper over cutover
failures, and CLAUDE.md's done-condition #4 requires `./test.sh` green
with no bootstrap reference. The lang suite is the only language-semantics
coverage in the repo (190+ claim assertions across the 11 files). Dropping
the lang phase from `test.sh` to declare victory would discard that
coverage — it would make the script green by deleting the check, not by
making the self-hosted toolchain capable. That is exactly the
"make-it-green-by-editing-the-test" failure the freeze rules warn against.

### The unblock (a future wave, not this one)

The self-hosted toolchain needs a faithful equivalent of
`sample <file> --all --json`. Candidate shapes, cheapest first:

1. **A `sample` mode for compiler.smt2 / a sibling `sample.smt2`** that,
   given a `.ev` source, iterates every top-level claim, solves each, and
   prints the `{"name":bool,...}` JSON `run-lang-tests.sh` already parses.
   This is the most faithful: same interface, same output contract.
2. **Per-claim emit + kernel sat-probe**: extend `compiler.ev` to accept a
   claim name (drop the "last bare-head wins" convention), emit each named
   claim as a minimal constraint-only `.smt2`, and define a kernel exit
   convention for sat vs unsat. Requires (a) claim selection in the
   compiler and (b) proving the exit-code proxy is faithful to bootstrap's
   `sample` verdicts on all 11 files. Larger and riskier than (1).

Until one of these lands, the lang phase cannot run on kernel +
compiler.smt2, and the cutover cannot make `./test.sh` green.

---

## Blockers 2-3 (kernel + conformance probes): IN PROGRESS

These two probes run every fixture through a full self-hosted compile
(`kernel compiler.smt2 < flat.ev`). Each compile takes minutes
(per-tick solve cost — wave 4h's "Blocker 5", structural/kernel-side),
so 100 kernel + 139 conformance fixtures take hours. They were still
running when this doc was written; results will be appended. They do not
change the verdict — Blocker 1 alone gates the cutover — but they tell the
coordinator how complete the self-hosted compiler is on the broader corpus.

<!-- KERNEL/CONFORMANCE RESULTS TO BE APPENDED -->

---

## Verdict

**Cutover not performed.** `bootstrap/` is intact; `test.sh` and
`scripts/evident-self` are unchanged; nothing was deleted. The
self-hosted compiler is feature-complete for the *compile-an-FSM* path
(test_hello semantic match, wave 4h) but the toolchain has **no `sample`
verb**, so the language-semantics test suite (`tests/lang_tests/`, the
project's single source of truth for language correctness) cannot run on
kernel + compiler.smt2. That capability gap is the next wave.
