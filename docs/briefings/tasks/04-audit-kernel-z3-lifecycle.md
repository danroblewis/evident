# Task: Audit kernel/ against the Z3-lifecycle invariant

## Why this matters

The coordinator just landed `docs/plans/architecture-invariants.md`,
which states the user-confirmed rules for how the kernel must
manage Z3 across an FSM's ticks:

1. The Z3 model is built ONCE per program.
2. Per tick, the ONLY allowed change is adding equality constraints
   to pin variables.
3. No tick may rebuild the Z3 model.
4. No tick may call `.simplify()` inside the tick loop.

**This audit confirms or contradicts whether the current `kernel/`
code obeys those rules.** We need to know before we build more
compiler work on top of it.

## Your task

This is **read-only research**. Do NOT modify any file in
`kernel/`, `bootstrap/`, `compiler/`, or `stdlib/`. You may write
your report to a new file under `docs/plans/`.

1. **Survey kernel Z3 touchpoints.** Find every Z3 call in
   `kernel/src/*.rs`. Common entry points:
   - `z3::Solver::new`, `solver.assert`, `solver.check`, `solver.get_model`
   - `model.eval`, `model.get_const_interp`
   - `Context::new`, `Config::new`
   - `simplify`, `Tactic`, `Goal`
   - Any raw `z3-sys` C API calls
   Use `rg "z3::|simplify|Solver|Context|Tactic" kernel/src/ -n` or similar.
   List the touchpoints with file:line refs.

2. **Trace the tick loop.** Identify the per-tick entry point
   (probably `kernel/src/tick.rs` or similar). Answer concretely:
   - Is the Z3 `Context` created once at program start, or per
     tick? Cite the construction site.
   - Is the `Solver` created once, or per tick? If per tick, is the
     SMT-LIB body re-asserted each time?
   - Where do state-carry equalities (`_x = …`) get asserted?
     `solver.push` + assert + `solver.check` + `solver.pop`? Or
     just naked re-asserts that accumulate?
   - Where does `last_results` get asserted? Same question.
   - Where does the SMT-LIB body get loaded? Is that load called
     once at startup or per tick?

3. **Check for `.simplify()` on the tick path.** Specifically:
   - Direct calls to `simplify()` on AST nodes or models.
   - `Tactic::new(_, "simplify")` constructions.
   - `Goal::new` followed by simplification.
   - Any of these on the per-tick code path = invariant violation.

4. **Determine if the kernel matches the invariants.**

   For each of the 4 invariants, write "MATCHES", "VIOLATES",
   or "AMBIGUOUS" with a one-sentence reason and a file:line
   citation. If "AMBIGUOUS," explain what would need to change in
   the code or test to disambiguate.

5. **If any invariant is violated:** describe what the violation
   costs in practice. Does the kernel re-parse the SMT-LIB body
   every tick (expensive)? Does it call simplify per tick (very
   expensive)? Does the Z3 context grow without bound? Quantify
   if you can, e.g. by counting tick-loop allocations or noting
   that the body is N KB.

## Output

Write your report to `docs/plans/audit-kernel-z3-lifecycle.md`.
Structure:

- One-paragraph summary at the top: "VERDICT: kernel matches |
  partially matches | violates the invariants. Recommended action: …"
- Per-invariant section with the MATCHES/VIOLATES/AMBIGUOUS
  designation and citation.
- "How the tick loop actually works" section with a code-level
  walkthrough (file:line, what each step does).
- If applicable: a short list of changes that would bring the
  kernel into compliance — but DO NOT IMPLEMENT THEM. The freeze
  still holds. This is a documented gap to surface to the user.

## Forbidden

- Editing any file under `kernel/`, `bootstrap/`, `compiler/`, or
  `stdlib/`.
- Adding new Python files.
- Cherry-picking from any other branch.
- Speculative refactoring of `kernel/` even on a private branch.

## Reporting back

Final message (terse):

- Branch pushed (`agent-04-audit-kernel-z3-lifecycle` or whatever
  coordinator.sh set up).
- One-sentence verdict.
- 4 lines: invariant #1 status, #2 status, #3 status, #4 status.
- Path to the written report.
- Cite at minimum `docs/plans/architecture-invariants.md` and any
  `legacy-python/docs/*` files you referenced.

Do NOT paste the report inline. The coordinator reads files.
