# Task: Find + extract the macro-finder functionizer

## Why this exists

We have been making implementation decisions based on Z3 solve
performance — A vs B vs E pin mechanisms, FTI vs cons-list, etc.
The user has flagged that **the relevant performance is
post-functionalization**, not pre. A functionizer turns FSM tick
bodies (and ideally entire FSMs) into compiled functions; once
that lands, what looks slow in Z3 today is irrelevant if the
shape functionizes cleanly. So we need the functionizer in scope
as a *known future capability* informing every implementation
choice.

User quote:

> *"We are spending time trying to do performance tests and
> optimize things, and agents are making decisions based on
> performance, but the performance will change. Like using Cons
> cells instead of other things. The questions the agents are
> asking are about performance, not about correctness. We actually
> care about more than correctness. For FSM's specifically, we
> care about how well the Evident models can be turned into
> functions by our functionizer, and we would choose our
> implementation details based on what can be functionized."*

Three functionizers existed in older codebases:

1. **Z3 `macro-finder` version** — uses Z3's `macro-finder` tactic
   to identify S-expressions that can be promoted to functions.
   **This is the one we want.**
2. Symbolic Regression version. **Skip.**
3. LLM version. **Skip.**

The user notes ideally the entire FSM compiles to a function; if
that fails for some FSM, there are other strategies (presumably
partial functionalization, per-claim, per-tick, etc.).

## Your task

This is **research + extraction**. Read-only on the old branches;
new files are docs and possibly a `legacy-python/` reference
import.

### Step 1: locate the right branch

The user said the code is "further back in a different branch I
think" — older than `tiny-runtime`. Survey:

```bash
git branch -r | head -100
git log --all --since='3 years ago' --oneline --first-parent | head -200
# Look for branches with names containing: functionize, jit,
# macro, compile, native, codegen
git log --all --grep='macro.?finder\|functionize\|jit' --oneline | head -50
```

Once you identify candidate branches, check each out into a tmp
worktree and look for files named `functionize*`, `jit*`,
`macro_finder*`, `compile_to_function*`, or similar. The
project memory in `~/.claude/projects/-Users-danroblewis-evident/memory/`
mentions `functionize/` directory and a "Mario JIT" effort —
those memory entries cite specific branches/sessions. Check them.

If nothing surfaces, document the search exhaustively and stop
(write `docs/plans/blocked-functionizer-search.md`).

### Step 2: identify the macro-finder version specifically

Once you find functionizer code, distinguish the three variants.
Indicators for the macro-finder version:

- Imports / calls to Z3's `macro-finder` tactic (`Tactic("macro-finder")`,
  `Z3_mk_tactic`, etc.).
- Logic about identifying ground-equality assertions that define a
  symbol as a function of its arguments.
- Output: a function definition that can be applied where the
  symbol was previously a constraint.

NOT macro-finder:

- Symbolic regression: searches for closed-form expressions over a
  hypothesis space. References "regression," "fitness," "expression
  trees."
- LLM: references model APIs, prompting, or external model calls.

### Step 3: extract

Copy the macro-finder functionizer's source files to
`legacy-python/functionizer/` (or `legacy-rust/functionizer/`
depending on language), following the pattern set by the
`legacy-python/docs/` reference import (read-only, marked frozen,
not on any critical path). DO NOT bring in symbolic-regression or
LLM variants — drop them.

Also copy any test fixtures + the design doc(s) that explain
the macro-finder approach, if they exist on the source branch.

### Step 4: write the integration design doc

Write `docs/plans/functionizer-integration.md` covering:

1. **What the macro-finder version does** in 1-2 paragraphs, with
   the key Z3 calls cited.
2. **How it would wire into the current architecture.** Today the
   kernel does: parse SMT-LIB → `.simplify()` → loop (per-tick
   pin + solve). The functionizer would slot in:
   - After parse + simplify, before the loop: try to convert as
     many tick-body assertions as possible into function
     definitions.
   - Per tick: invoke the compiled functions, fall back to Z3 for
     anything that didn't functionize.
   Describe the seam concretely (which `tick.rs` functions
   change, what new structures are needed).
3. **What about a given FSM matters for "functionizes cleanly"** —
   shape guidelines so future sessions can make
   functionizability-aware implementation choices.
   - The user's example: cons-cells vs Z3 sequences — which one
     does the macro-finder cope with better?
   - Other shape questions: nested datatypes, recursion depth,
     `match` arity, quantifiers.
4. **Estimated effort + risk.** Is this 1 session, 10 sessions,
   30? What might break?
5. **Whether to implement now or defer.** If the answer is
   "implement now," what's the first PR-sized chunk?

### Step 5: capture the principle in invariants

Append a section to `docs/plans/architecture-invariants.md`:

```
## Functionizability over Z3-fast: the implementation-choice principle

When choosing between two implementation shapes that are both
correct, prefer the one that functionizes more cleanly over the
one that solves faster in Z3 today. The functionizer (macro-finder
version, see `docs/plans/functionizer-integration.md` and
`legacy-{python,rust}/functionizer/`) is the post-load optimizer
we trust; what's slow in Z3 today becomes a constant cost after
functionization if the shape is right.

Concretely:
- Prefer in-Evident datatype cons-lists over Z3-native Seqs when
  the data is bounded — cons-lists functionize as recursive
  function definitions; Z3 Seqs are opaque to the functionizer.
- Prefer fixed-arity match arms over variadic Seq operations for
  the same reason.
- [Add to this list as we discover more.]
```

## Acceptance

1. `legacy-python/functionizer/` (or `-rust/`) exists with the
   macro-finder source — NOT the SR or LLM variants.
2. `docs/plans/functionizer-integration.md` exists, 1–3 pages,
   covering the 5 sections above.
3. `docs/plans/architecture-invariants.md` updated with the
   functionizability principle.
4. `legacy-python/README.md` (or its equivalent) updated to
   mention the new functionizer reference.
5. Diff touches only `legacy-*/`, `docs/plans/`, and
   possibly `legacy-python/README.md`. Nothing in `kernel/`,
   `bootstrap/`, `compiler/`, `stdlib/`.
6. `./test.sh` green (unchanged from baseline — this task adds
   docs and reference, no code).

## Forbidden

- Bringing in Symbolic Regression or LLM functionizer variants.
- Editing `kernel/`, `bootstrap/`, `compiler/`, `stdlib/`.
- Implementing the functionizer in this task — that's a
  follow-up.
- Modifying anything on a source branch (purely read-only on the
  old branches).
- Adding new Python to the main repo's `scripts/` or `tests/`.
  The `legacy-*/` import is exempted (it's read-only reference).

## Reporting back

- Branch pushed.
- Source branch you found + tip commit hash.
- Filenames extracted to `legacy-*/functionizer/`.
- Recommendation: implement now / defer / split.
- `./test.sh` final line.
- `scripts/check-deletable.sh` blocker count (note: this task
  may slightly increase Python or Rust counts if `legacy-*/`
  files match patterns — check the script's exclusions and
  update them if needed).
- Cite the source branch's relevant files explicitly.

Be terse. The coordinator reads `docs/plans/functionizer-integration.md`
directly.
