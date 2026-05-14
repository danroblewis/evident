# test_21_mario — entity-based Mario demo

Multi-FSM platformer over a hand-rolled entity system. The level
(platforms + enemy spawn data) and the physics are both expressed
as constraints over Seq-of-record types; the player + enemies are
Mover records; collision is a relation over the platform
collection (no hardcoded floor / wall coordinates).

## Files

- `main.ev` — the whole program. Entity types, a `Level` claim
  whose body asserts the layout constraints (in-bounds,
  non-overlap, jump-reachability, spread), two FSMs (`game` for
  physics, `display` for input + rendering) that share `Level`
  via `..Level`.

## Level as a constraint problem

`Level`'s body has no hand-pinned coordinates for the elevated
platforms. Instead, `plat_x[i]` and `plat_y[i]` are free Int
Seqs Z3 picks subject to:

- **In-bounds:** every platform fits inside the world horizontally
  and sits in a band above the ground and below the ceiling.
- **Non-overlap:** pairwise separation on at least one axis by
  at least `MIN_GAP` pixels.
- **Reachability:** every elevated platform is jump-reachable
  from the ground OR from another platform below it whose
  x-range overlaps (cheap-arc approximation of the player's
  jump parabola given `grav` and `jump_strength`).
- **Spread:** at least one platform high, one low; one on the
  left half, one on the right. Stops Z3 from clustering them.

The cached solver returns the same valid layout on every
per-tick query, so the game and display FSMs see identical
platforms even though they each run their own Z3 instance.
To get a different layout, tweak the constraints (or pass a
different `EVIDENT_Z3_ARITH_SOLVER` / seed env var).

## Runtime gaps the file works around

- **Set-of-records is unsupported.** `Body(...) ∈ platforms` as a
  set-membership declaration would be the natural way to define
  platforms; today we use `Seq(Body)` with `#platforms = N` plus
  `platforms[i] = Body(...)` pins. See COUNTEREXAMPLES.md #15.
- **3-level nested writes through `world_next` are dropped.** Each
  guarded enemy-physics implication assigns the whole `Mover`
  record at once instead of `nxt.pos.x = …`. See
  COUNTEREXAMPLES.md #23.

## Future shape

If Mario grows to multiple levels (level 1 → boss → level 2),
the natural move is a `LevelGen` FSM that owns `world.platforms`
and rewrites it on level transitions. Game/display subscribe to
the world field. That'd require addressing the
cardinality-as-write classification bug and deciding the
multi-writer semantics for `world.platforms`.
