# test_21_mario — entity-based Mario demo

Multi-FSM platformer over a hand-rolled entity system. The level
(platforms + enemy spawn data) and the physics are both expressed
as constraints over Seq-of-record types; the player + enemies are
Mover records; collision is a relation over the platform
collection (no hardcoded floor / wall coordinates).

## Files

- `main.ev` — entry point. Declares the entity types, the world,
  the level data, and two FSMs (`game` for physics, `display` for
  input + rendering).
- `level_gen.ev` — constraint-based layout generator. A standalone
  `LevelGen` claim that picks valid (x, y) coordinates for the
  three elevated platforms subject to in-bounds, non-overlap,
  reachability, and layout-spread constraints.

## Regenerating the platform layout

`main.ev`'s `platforms[1..3]` positions come from solving `LevelGen`.
To pick a new layout (e.g. after editing constraints in
`level_gen.ev`):

```
evident query examples/test_21_mario/level_gen.ev LevelGen --json
```

The output looks like:

```
{"satisfied": true, "bindings": {
    "plat_x": [320, 38, 480],
    "plat_y": [350, 236, 262],
    ...
}}
```

Hand-paste the values into `main.ev`'s `platforms[1..3]` definitions
(both FSMs — they have to match, see runtime gap below). Z3 picks
the same layout on every run unless you pass a different seed.

## Runtime gaps the file works around

- **Set-of-records is unsupported.** `Body(...) ∈ platforms` as a
  set-membership declaration would be the natural way to define
  platforms; today we use `Seq(Body)` with `#platforms = N` plus
  `platforms[i] = Body(...)` pins. See COUNTEREXAMPLES.md #15.
- **`..Passthrough` doesn't propagate to the ∀-unroller.** The
  platform pins have to be inlined into both game and display
  bodies (duplicated), because moving them into a separate claim
  reachable via `..Level` would leave them invisible to
  `collect_pinned_ints`. See COUNTEREXAMPLES.md #22.
- **3-level nested writes through `world_next` are dropped.** Each
  guarded enemy-physics implication assigns the whole `Mover`
  record at once instead of `nxt.pos.x = …`. See
  COUNTEREXAMPLES.md #23.

## Future shape

When the runtime gaps land, `level_gen.ev` becomes the only
source of truth: `main.ev` would import it directly, the FSMs
would `..Level` and access `platforms[i]` through the passthrough,
and Z3 would solve the layout once at program load. No regen
step, no duplicated pin block.
