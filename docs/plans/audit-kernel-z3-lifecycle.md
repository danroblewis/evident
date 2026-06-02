# Audit: `kernel/` against the Z3-lifecycle invariant

**VERDICT: the kernel VIOLATES the Z3-lifecycle invariants.** Three of the
four rules in `docs/plans/architecture-invariants.md` ("Z3 model lifecycle")
are violated; only the `.simplify()` ban is honored. Concretely, the kernel
creates the Z3 *context* once but **re-parses the entire SMT-LIB body and
builds a fresh `Z3_solver` on every tick** (`kernel/src/tick.rs:62-111`),
discarding all asserted constraints and solver state between ticks. The
header comment in that file states this outright as the MVP design:
"fresh `Z3_solver` per tick, fresh parse of full SMT each tick"
(`kernel/src/tick.rs:5-7`).

**Recommended action:** Treat this as a documented, user-surfaced gap, not a
silent assumption. The current behavior is *functionally correct* (state-carry
and `is_first_tick` work as specified) but *architecturally wrong* under the
invariants and pays a per-tick re-parse cost that grows with program size —
which directly matters once the body is `compiler.smt2`. The kernel is FROZEN;
bringing it into compliance requires a written proposal + explicit user
approval per the freeze rules. A compliance sketch is at the bottom of this
doc; it is NOT implemented here.

---

## Per-invariant designation

Invariants are quoted from `docs/plans/architecture-invariants.md` §"Z3 model
lifecycle".

### Invariant #1 — "The Z3 model is built ONCE." → **VIOLATES**

The Z3 *context* is created once (`kernel/src/tick.rs:53-55`, before the tick
loop) and reused. But the invariant's "model" means the program's loaded
constraint system ("loaded into a Z3 context at startup — by parsing the
program's SMT-LIB"). That parse/load happens **inside** the tick loop: every
iteration calls `Z3_parse_smtlib2_string` on the full body
(`kernel/src/tick.rs:93-97`) and asserts the results into a freshly created
solver (`kernel/src/tick.rs:86-87`, `108-111`). The constraint system is
therefore rebuilt once per tick, not once per program.

### Invariant #2 — "The model is REUSED across all ticks. Per tick, the ONLY allowed change is adding equality constraints to pin variables." → **VIOLATES**

Nothing is reused between ticks except the bare context. Each tick:

- rebuilds the full SMT text `full = src + carry-asserts + last_results + is_first_tick` (`kernel/src/tick.rs:64-84`),
- re-parses **all** of it, not just the equality pins (`kernel/src/tick.rs:93-97`),
- asserts every parsed top-level AST into a new solver (`kernel/src/tick.rs:107-111`).

The state-carry equalities (`(assert (= _x …))`, `kernel/src/tick.rs:67-75`),
the `last_results` pins (`kernel/src/tick.rs:76-79`), and the `is_first_tick`
pin (`kernel/src/tick.rs:80-84`) are appended as **text** to the body and
re-parsed — not asserted incrementally onto a persisted solver. So the
per-tick change is "re-assert the entire program," which is strictly more than
"add equality constraints."

### Invariant #3 — "No tick may rebuild the model." → **VIOLATES**

Each tick rebuilds the model: a fresh `Z3_mk_solver` (`kernel/src/tick.rs:86`)
plus a full re-parse (`kernel/src/tick.rs:93`). No `Z3_solver_push` /
`Z3_solver_pop` exists anywhere in `kernel/src/` (confirmed by grep — the only
solver calls are `Z3_mk_solver`, `Z3_solver_inc_ref/dec_ref`, `Z3_solver_assert`,
`Z3_solver_check`, `Z3_solver_get_model`). The fresh-solver-per-tick pattern is
a rebuild every iteration.

### Invariant #4 — "No tick may call `.simplify()` on the model." → **MATCHES**

No `simplify`, `Tactic`, or `Goal` appears anywhere under `kernel/src/`
(confirmed by grep over all four source files). The tick path does
`Z3_parse_smtlib2_string` → `Z3_solver_assert` → `Z3_solver_check` →
`Z3_solver_get_model` → `Z3_model_eval`, with no simplification step. This is
the one invariant the current kernel honors.

---

## How the tick loop actually works (code-level walkthrough)

Entry: `main.rs:35` calls `tick::run(&src, &manifest)`, which delegates to
`run_inner` (`kernel/src/tick.rs:52`). The `.smt2` body (`src`) is read once
in `main.rs:19` and never reloaded; the manifest is parsed once
(`main.rs:27`). So *file* loading is once-per-program. The Z3 work is not.

Setup, once (`kernel/src/tick.rs:53-59`):
- `Z3_mk_config` → `Z3_mk_context` → `Z3_del_config`. **Context is created
  once and lives for the whole run.**
- `prev_state`, `prev_results`, `is_first` initialized.

Per-tick loop body (`kernel/src/tick.rs:62-181`), for `tick` in `0..100_000`:

1. **Rebuild the full SMT string** (`64-84`): start with `src`, then if not
   the first tick append `(assert (= _<name> <prev value>))` for each carried
   state field (`67-75`); always append `(assert (= last_results__len N))` and
   per-index `(assert (= (select last_results i) <Res>))` (`76-79`); append
   `(assert is_first_tick)` or `(assert (not is_first_tick))` (`80-84`).
2. **Create a fresh solver** (`86-87`): `Z3_mk_solver(ctx)` + `inc_ref`.
   Nothing from the prior tick's solver is carried over.
3. **Re-parse the entire body** (`89-105`): `Z3_parse_smtlib2_string` over the
   whole `full` string. On parse failure the kernel tears down and returns an
   error (`98-105`).
4. **Assert every parsed top-level AST** into the fresh solver (`107-111`):
   loop over `Z3_ast_vector_get` → `Z3_solver_assert`.
5. **Check** (`113-124`): `Z3_solver_check`. `Z3_L_FALSE` → exit 2 (UNSAT);
   non-`Z3_L_TRUE` → error (UNKNOWN).
6. **Get model + read state** (`126-133`): `Z3_solver_get_model` then
   `read_state_var` for each manifest field. Primitive types decode directly;
   anything else is walked as a Datatype (`read_state_var:294`,
   `decode_datatype_value:315`).
7. **Walk effects** (`136-158`): read `effects__len`, clamp to `max_effects`,
   then for `i` in `0..len`: `Z3_mk_select` the `i`-th effect, `Z3_model_eval`
   it, and `dispatch_effect` (`220`). Effects: `Exit` (→ halt code),
   `ReadLine`/`ReadFile`/`WriteFile` (built-ins), `LibCall` (libffi via
   `libcall::call`). Each non-exit effect yields a `Res` pushed to
   `new_results`.
8. **Exit / stuck checks** (`160-176`): if an `Exit` was emitted, tear down
   and return its code. Otherwise compute `stuck` = (not first tick) ∧ all
   state fields unchanged (`168-169`); if stuck, exit 1.
9. **Thread state forward** (`178-180`): `prev_state = new_state`,
   `prev_results = new_results`, `is_first = false`.
10. **Teardown of per-tick Z3 objects**: `Z3_model_dec_ref` (`170`) and
    `Z3_solver_dec_ref` (`171`) every tick. The context is only deleted on a
    terminal path (`117/122/163/174/183`).

So the *only* thing reused across ticks is the `Z3_context` (its sort/symbol
interning). The solver, the parsed AST vector, and the model are all created
and dropped every tick.

---

## What the violation costs in practice

The functional behavior is correct: re-parsing `body + fresh equality pins`
into a fresh solver yields the same SAT assignment that incremental assertion
would. The cost is performance, and it scales with the thing this project is
building toward.

1. **Full re-parse every tick.** `Z3_parse_smtlib2_string` rebuilds every AST
   node — all `declare-datatypes` (the Effect/Result enums, any user enums,
   Seq cons-cell datatypes), every claim's constraints, the effects
   machinery — from text, once per tick (`kernel/src/tick.rs:93`). Cost is
   ≈ `O(body_size)` per tick. For an FSM that runs `T` ticks, that is `T`
   parses of the whole body. The tick limit is 100,000
   (`kernel/src/tick.rs:61`), so a long-running FSM over a large body pays the
   parse 100,000 times.

2. **The body size is about to explode.** Today's kernel fixtures are small,
   so the re-parse is cheap in absolute terms. But the deletion target is
   `kernel + compiler.smt2`, where the body is the entire self-hosted
   compiler. Re-parsing a multi-hundred-KB `compiler.smt2` on every tick of a
   compile is the dominant cost path, and it is pure waste — the body is
   identical every tick; only the equality pins change.

3. **No incremental solving.** A fresh `Z3_mk_solver` each tick
   (`kernel/src/tick.rs:86`) throws away all learned clauses / lemmas from the
   prior solve. Incremental solving (assert body once at base scope, then
   push/pop the per-tick equalities) is exactly the workload Z3's incremental
   mode is built for. The current design forfeits it entirely.

4. **Per-tick allocation churn.** Each tick allocates a fresh `String`
   (`with_capacity(src.len() + 256)`, `kernel/src/tick.rs:64`), a `CString`
   copy of it (`89`), a fresh solver, and a fresh AST vector. All are dropped
   at tick end. This is bounded per tick but multiplied by tick count.

The context being reused means the violation is "re-parse + re-solve," not the
even-worse "new context per tick" (which would also re-intern all sorts). So
the gap is real but the fix is localized to the solver/parse lifecycle, not
the context lifecycle.

---

## Compliance sketch (NOT implemented — freeze holds; needs user approval)

For the user's reference only. Per CLAUDE.md and the freeze rules, `kernel/`
edits require a written proposal in `docs/plans/` plus explicit per-edit
approval. This is the shape such a proposal would take:

1. **Parse the body once, before the tick loop.** Move
   `Z3_parse_smtlib2_string` over `src` (without the per-tick equality pins)
   to a single call before `for tick in …`. Keep the resulting AST vector.
2. **Create one persisted solver before the loop.** `Z3_mk_solver(ctx)` once;
   assert the parsed body ASTs once at base scope.
3. **Per tick, push → assert equality pins → check → pop.** Replace the
   "rebuild full text + fresh solver + full re-parse" block
   (`kernel/src/tick.rs:64-111`) with: `Z3_solver_push`; build only the
   `_<name>=…`, `last_results`, and `is_first_tick` equality ASTs (either by
   parsing a *tiny* per-tick string of just those asserts, or by constructing
   them via the Z3 C API directly) and `Z3_solver_assert` them; `Z3_solver_check`;
   read model + effects as today; `Z3_solver_pop` to discard the tick's pins
   before the next iteration.
4. **Keep `is_first_tick` / state-carry semantics identical.** The only
   behavioral contract is "the body holds + these equalities hold this tick";
   push/pop preserves it exactly while reusing the parsed body and the solver's
   learned state.

This brings #1, #2, #3 into compliance (#4 already passes) and removes the
per-tick full-body re-parse, which is the dominant cost once the body is
`compiler.smt2`. It is intentionally left unimplemented.

---

## References consulted

- `docs/plans/architecture-invariants.md` — the four Z3-lifecycle invariants
  audited here (§"Z3 model lifecycle").
- `kernel/src/tick.rs` — the tick loop (the subject of the audit).
- `kernel/src/main.rs`, `kernel/src/manifest.rs`, `kernel/src/libcall.rs` —
  the rest of the kernel; confirmed no Z3 model/solver work outside `tick.rs`.
- `legacy-python/docs/runtime-architecture.md` — longer-form rationale for
  "Z3 is a library, not a runtime" and the trampoline/tick model the invariants
  abbreviate.
- `CLAUDE.md` / `docs/briefings/foundation.md` — freeze rules constraining this
  to a read-only audit with no `kernel/` edits.
