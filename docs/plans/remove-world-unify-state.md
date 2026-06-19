# Remove "world", unify FSM record-state onto the `_var` prev-tick mechanism

**This is the keystone task.** Do it before more `Δ`, FTI, or phase-portrait work — it
unblocks all three.

## Goal
1. The word **"world" is not meaningful to Evident** — it was a mistake (a reserved name
   left over from the dead multi-FSM era). Remove **every** use of "world" from `runtime/`.
2. **Unify all record-typed FSM state onto the single `_var` prev-tick mechanism**:
   `_var` = previous tick, `var` = current tick. Delete the legacy `var`/`var_next`
   state-pair (`state`/`state_next`, `world`/`world_next`) and the `unify_world_syntax`
   rename pass.

## Why (keep this — it is the whole justification)
Today an FSM has **two parallel mechanisms** for "a value that persists tick-to-tick":
- **Scalars** use the clean form: `_x` (previous), `x` (current), via
  `inject_prev_tick_decls`; the runtime carries `x → _x`. There is no `x_next`.
- **Records** use a legacy **state-pair**: `state`/`state_next`, `world`/`world_next`, via
  `inject_fsm_params` + `resolve_fsm`/`seed_state`; the runtime seeds `state`/`world` from
  the *prior* tick's `state_next`/`world_next`. This is the multi-FSM *writer pattern* —
  and multi-FSM is dead (single-FSM only now).
- `unify_world_syntax` (in `encode/lower.rs`) is a **pure rename** bridging the nice
  `_world`/`world` surface to the legacy pair: `_world.X → world.X`, `world.X → world_next.X`.

"world" is just a reserved name for *one* record-state instance; `state`/`state_next` is the
identical pattern. The split is incidental complexity dragged from the multi-FSM era.

### What the split breaks (this is why it's the keystone, not just tidy-up)
1. **`Δ` on record fields fails.** `Δworld.X` desugars to `world.X − _world.X`, but
   `unify_world_syntax` mangles it into a `world_next` subtraction the state-pair detector
   doesn't recognize → the constraint (and even a plain seed like `world.x = 10`) is
   **silently dropped**. `Δx` on *scalars* works precisely *because* scalars use `_var`.
   (Verified: reordering `desugar_delta` before `unify_world_syntax` stops the crash but the
   constraints still drop — the pair machinery is the root cause, not the pass order.)
2. **FTIs can't get their time-shift.** A `File` FTI is record-typed state carried
   tick-to-tick; its design needs `_file.offset` (previous) / `file.offset` (current). That
   doesn't exist because records go down the `_next`-pair path. **Unifying record state onto
   `_var` *is* the FTI time-shift.**

So one change removes legacy cruft, fixes `Δ`-on-records, **and** unblocks the FTIs.

## Target model
- An FSM's state is one (or more) record-typed variable(s).
- `_var.field` = previous tick's value (given to the solve), handled exactly like `_x`.
- `var.field` = current tick's value (solved for).
- The runtime carries `var → _var` per-field across ticks — the same carry scalars get.
- **No `var_next`, no `state`/`state_next`/`world`/`world_next`, no `unify_world_syntax`.**
- Expressiveness is preserved: `_var` (prev) + `var` (current) references both old and new
  in one solve, same as the pair did.

## Tasks
1. **Delete `unify_world_syntax`** (`encode/lower.rs`) and its call in `session/mod.rs`.
2. **Unify the prev-tick mechanism for records.** Make `inject_prev_tick_decls` handle
   `_var.field` (record-field previous) the way it handles `_x`. (It reportedly already does
   per-field record prev — confirm and extend if needed.)
3. **Migrate the runtime's state-carrying.** In `trampoline.rs` (`resolve_fsm`,
   `seed_state`, `run_loop`) and `encode/lower.rs` (`inject_fsm_params`): replace "seed
   `state`/`world` from prior `state_next`/`world_next`" with "carry the state record
   `var → _var` via the prev-tick path." Remove
   `state_var`/`state_next_var`/`world_var`/`world_next_var`/`world_type` from `MainShape`.
   New FSM shape: a state record var + its `_var` prev, `last_results`, `effects`.
4. **Purge the word "world"** from `runtime/` entirely — string literals, reserved-name
   checks, field/var names, comments. `grep -rn "world" runtime/src` should be **zero**.
5. **Migrate demos + stdlib + packages.** Rewrite every `.ev` using `world`/`world_next` or
   `state`/`state_next` to the `_var`/`var` record form. (`grep -rln "world\|_next" examples
   stdlib packages --include=*.ev`.)
6. **Decide the state-declaration surface.** With `state`/`state_next` gone, how does an FSM
   name its state? Likely just `s ∈ MyState` with `_s.field` (prev) / `s.field` (current).
   Keep it consistent with the scalar `_x` story.

## Verification
- `./test.sh` green — every migrated demo ticks identically. This is the contract.
- A `Δ`-on-record test passes: `Δs.pos = s.vel` lowers and solves like `Δx`.
- The phase-portrait spring (branch `phase-portraits`) rewrites to `Δs.pos = s.vel` and runs.
- A pre-existing stateful demo (counter / mario / prev_tick) behaves identically after migration.

## Where this sits (context for the next session)
**Done so far (all on `main` unless noted):**
- `fti` keyword (`Keyword::Fti`; lexer/parser/ast) — today behaves as a *type* whose
  invariants constrain the solution space (proven: a `File` with `offset<0` is UNSAT). No
  fsm-nature yet. Commit `564d0de`.
- `Δ` tier 1: primitive scalar **forward difference** `Δe ≡ e − _e`. Works for scalar FSM
  vars; `examples/test_23_difference.ev`. Commit `7f4a0fa`.
- Phase-portrait exploration on branch **`phase-portraits`** (`2f1d470`, NOT merged): the
  damped-spring difference equation and the spiral *math* are correct (verified; a spiral PNG
  was rendered externally), but the live SDL render is blocked because **a `Seq` element
  won't bind into SDL draw coordinates** — so you can't redraw an accumulated trail or a
  vector field. Separate gap, also needed for the real on-screen portrait.

**After this keystone:**
- `Δ` tier 2 (record-componentwise `Δrec ≡ Rec(Δf1, …)`) and tier 3 (custom type-declared
  `Δ` via a `Δ = …` type member — for angles/clamped/semantic deltas; open Q: can `ΔT` be a
  different type than `T`?).
- The **FTI state machine** (the long design thread): `File` = `Open`/`Seek`/`Transfer`/`Close`;
  foreign-owned state (handle/offset/size) **pinned from effect results via a generalized
  `Bind`, carried via `_var`, never solved** (the solver must not invent a file handle);
  every LibCall resolves synchronously within the tick (one-tick result latency is fine);
  read/write are one `Transfer` parameterized by direction; the elegant **mmap** model holds
  the file contents in a `Seq` so read/write become one relation solved in two directions.
- The phase-portrait renderer (also needs the `Seq`→SDL-coordinate binding fixed).

## Scope / risk
Touches the runtime's state-carrying core (`trampoline.rs` tick loop + `encode/lower.rs`
inject passes) and every stateful demo. Substantial but high-value: removes legacy multi-FSM
cruft, fixes a real bug (`Δ`+records), and unblocks the FTIs. Migrate demos as you go;
`./test.sh` is the regression oracle. Fallback tag if anything goes wrong: `v0.0.1`.
