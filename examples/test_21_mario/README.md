# test_21_mario — entity-based Mario side-scroller

Single-FSM platformer. One `fsm main(world ∈ World)` runs three
concerns in declaration order each tick, all coordinating through the
shared `World` record (`_world.X` reads the previous tick, `world.X`
writes this tick):

- **Input poll** — `sdl_pump_events` + three `ReadByte`s of the SDL
  keyboard state; writes `world.keys`. The key results come back as
  `last_results[1..3]` on the next tick (the four input effects lead
  the `effects` Seq).
- **Physics + game logic** — gravity, platform landing, camera follow,
  per-enemy stomp/side collision, coin pickup, lives/death/respawn,
  Big-Mario growth, end-of-level flag. Writes `world.player`,
  `world.enemies`, `world.coins`, `world.camera_x`, `world.lives`,
  `world.dead`, `world.coin_count`, `world.won`, `world.is_big`.
- **Render** — draws the platforms, Mario, enemies, coins, flag,
  castle, and HUD via `win.draw_rect` / `win.render_fill_rect`, then
  `render_present`. Writes `world.tick`.

The frame's effects are one ordered `Seq(Effect)`: the four input-poll
effects first, then the full render chain, then `delay`/`present` and
the tick-240 `Println("mario done") + Exit(0)`.

## Files

- `main.ev` — the single `main` FSM plus the level/entity types
  (`Level`, `World`, `Mover`, `AABB`, `Body`, `Coin`, `MarioSprite`).

## What's a `type` vs a `claim` here?

- **`type Level`** is the level VALUE — a noun. Constants (`PLAT_W`,
  `GROUND_Y`, …) and the materialized `platforms`, `e_init`, `c_init`
  Seqs. Its body has only local invariants (each `platforms[i]` is a
  literal); no external dependencies.
- **`type World`** is the shared mutable state the FSM threads across
  ticks.
- **`type MarioSprite`** builds the four-rect Mario sprite from a
  position + `is_big` flag.

## Death, coins, growth

- **Death**: a side-hit on an un-stomped enemy or falling below the
  screen sets `world.dead`; the next tick respawns Mario at x=100 and
  decrements `world.lives`.
- **Stomp**: landing on an enemy from above (`intent_vy > 0`,
  was-above) marks it dead (`pos = (-1000, -1000)`) and bounces Mario.
- **Coins**: AABB overlap marks a coin collected; collected coins
  render off-screen. Three coins → Mario goes Big.
- **Win**: touching the end-of-level flag sets `world.won` (green
  banner in the HUD).

The demo exits cleanly at frame 240 (`mario done`).
