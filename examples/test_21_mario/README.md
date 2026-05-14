# test_21_mario — entity-based Mario with constraint-generated levels

Multi-FSM platformer. Three FSMs:

- **`level_gen`** owns the level state in `world.plat_x` /
  `world.plat_y` / `world.level_idx`. On tick 0 (and on every
  level-beat transition) it solves the active level's constraints
  and writes the platform positions to world; otherwise it freezes
  them to the previous tick's values.
- **`game`** owns `world.player` / `world.enemies`. Reads
  `_world.plat_x` to know where the platforms are.
- **`display`** owns `world.keys` / `world.tick`. Polls the
  keyboard, renders the scene.

## Files

- `main.ev` — all three FSMs, the level type, and the per-level
  claims (`Jumpable`, `Level1`, `Level2`).

## What's a `type` vs a `claim` here?

- **`type Level`** is the level VALUE — a noun. Constants
  (`PLAT_W`, `GROUND_Y`, …), free placement vars (`plat_x`,
  `plat_y`), the materialized `platforms` and `e_init` Seqs.
  The body has only local invariants (each `platforms[i]` is
  built from the Level's own `plat_x[i]` / `plat_y[i]`); no
  external dependencies.
- **`claim Jumpable`** is a property OF a Level: every platform
  fits, none overlap, every elevated platform is jump-reachable.
  Generic — works on any Level via names-match.
- **`claim Level1` / `claim Level2`** are level-specific
  predicates: each composes `Jumpable` plus its own extras
  (spread, vertical staircase, etc.). Adding a new level =
  add a `Level3` claim and one dispatch line in `level_gen`.

## How `level_gen` switches levels

```
beat ∈ Bool = (¬is_first_tick ∧ _world.player.pos.x ≥ 580)

world.level_idx = (is_first_tick ? 0
                   : (beat ? (_world.level_idx + 1) mod 2
                      : _world.level_idx))

level_changed ∈ Bool = (is_first_tick ∨ beat)

world.level_idx = 0 ⇒ Level1
world.level_idx = 1 ⇒ Level2

¬level_changed ⇒ (∀ i : plat_x[i] = _world.plat_x[i] ∧ …)
```

Two things make the level stable:
- **Dispatch is guarded** — only the active `LevelN`'s
  constraints fire (others are vacuous because their
  antecedent is false).
- **Freeze constraint** — when the level didn't change, the
  current tick's `plat_x[i]` is pinned to the previous tick's,
  so Z3 can't pick a different valid solution.

When the player walks off the right edge (`pos.x ≥ 580`),
`beat` flips true → `level_idx` increments → the next tick the
freeze is OFF, the new `LevelN`'s constraints take over, Z3
picks a fresh layout, `world.plat_x` updates. Game and display
read the new positions via `_world.plat_x`.

## Adding a level

1. Write `claim LevelN` with `..Jumpable` + your specifics.
2. Add `world.level_idx = N ⇒ LevelN` to `level_gen`.
3. Bump the modulus in the `level_idx` transition.

That's the whole edit.

## Runtime gaps the file works around

- **Set-of-records is unsupported.** See COUNTEREXAMPLES.md #15.
- **3-level nested writes through `world_next` are dropped.**
  Enemy physics writes the whole `Mover` per implication branch.
  See COUNTEREXAMPLES.md #23.
