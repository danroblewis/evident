# Round 15 — Positional claim-call inlining

**Outcome:** STRUCTURAL CHANGE. The function-izer pre-pass inlines
`Constraint(Call(claim_name, args))` body items by substituting
args for the called claim's params — same logic the Z3 translator
already does at the assert step, hoisted to AST level.

This unblocks Mario's `keyboard` FSM at the gate. The next blocker
appears in the slow-path classifier (the world_next-as-output
carry-through issue, deferred to Round 16+).

## What changed

### `inline_positional_calls(body, claim_lookup)` pre-pass

`functionize.rs` gains a new top-level helper:

```rust
pub fn inline_positional_calls(
    body: Vec<BodyItem>,
    claim_lookup: &dyn Fn(&str) -> Option<SchemaDecl>,
) -> Vec<BodyItem>
```

For each `Constraint(Call(name, args))`:
1. Look up `name` as a known claim.
2. Take its first N Memberships as positional params.
3. Substitute each param name with the corresponding arg
   expression throughout the called claim's body items.
4. Append the substituted body items (minus the param Memberships)
   in place of the original Call constraint.

Bounded recursion via a visiting-set prevents `A → B → A` cycles
from blowing up.

### Pre-pass runs in try_functionize

The runtime's `try_functionize` clones the schema, replaces body
with the inlined version, and threads the new schema through both
`gate_diagnostics` and `try_extract_one_chain`. Everything
downstream sees the normalized body.

## Mario rejection progress

```
                Before                              After
display:    body Call sdl_delay                body Call win.set_draw_color
keyboard:   body Call sdl_pump_events          (gate passes; classify rejects
                                                world_next carry-through)
game:       Forall (non-static bounds)        Forall (non-static bounds)
level_gen:  non-Eq Binary op Implies          non-Eq Binary op Implies
```

Display moves from regular-claim-call rejection to **subschema-
call** rejection: `win.set_draw_color((220, 40, 60), out_eff)`
where `set_draw_color` is a subclaim of `SDL_Window`. That needs
field-rebinding (the receiver's leaves get aliased onto bare names
inside the subclaim body) — Round 16 work.

Keyboard reaches the gate but the classifier marks several
`world_next.*` fields as non-functional. These are world fields
that OTHER FSMs write (player.pos, plat_x, …) — keyboard doesn't
touch them, but they appear in the world type and the per-tick
`world_next` snapshot. The runtime's slow path handles this by
carry-through (`world_next.X = world.X` when keyboard doesn't
write X). The function-izer would need to implement the same
carry-through.

## Cross-example HITs

No change from Round 14 — every demo that hit the new
positional-inlining unblock also hit a deeper blocker (subschema
calls, world carry-through). The infrastructure paves the road but
doesn't land a new shipping win on its own.

## Test impact

- All 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=0/1`.
- All 119 conformance tests pass in both modes.

## Round 16 candidates

1. **World carry-through for FSMs** — fill in `world_next.X = world.X`
   for fields the current FSM doesn't write. Same logic the
   scheduler handles. Would unlock keyboard.
2. **Subschema-call inlining** — `win.subclaim(args)` with field-
   rebinding of the receiver's leaves onto bare names. Same logic
   as `runtime/src/translate/inline.rs::inline_subschema_call` at
   AST level. Would unlock display.
3. **Forall-over-Seq** (Mario game).
4. **Implies as guarded substitution** (Mario level_gen).

Recommend Round 16 = #1 (world carry-through), as it's the
narrowest patch with a real-world demo win attached (keyboard).
