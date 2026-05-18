# Mario — Delivered Features (autonomous run, May 17 2026)

Took Mario from "Mario sprite + 3 platforms + 2 enemies + ~30 fps" to a
recognizable side-scrolling platformer with the canonical SMB-1
mechanics. Plan: `super-mario-roadmap.md`.

## What ships now

### Visual / camera
- **Side-scrolling level** — 1920 px wide (3 screens @ 640).
- **Camera follows Mario** — clamped to [0, LEVEL_W - 640]. Platforms
  in world coords, display projects via `world.x - camera_x`.
- **End-of-level flag pole + flag + castle** at x=1800 (white pole, red
  flag triangle, grey castle with darker door).

### Geometry
- **7 platforms**:
  - 2 ground segments (with a pit gap at x ∈ [700, 800]).
  - 5 floating brown platforms across the level.

### Enemies
- **3 patrolling Goombas** (pink rects, 28×28) bouncing inside their
  patrol ranges.
- **Stomp interaction** — landing on top of an enemy from above kills
  it (off-screen) and bounces Mario upward.
- **Side collision** — touching enemy from side kills Mario.

### Mario state
- **Lives** — starts at 3. Decrements on enemy hit OR fall in pit.
- **Death + respawn** — Mario respawns at (100, 100); game continues.
- **Game over** — 0 lives → Mario exits.
- **Big/Small form** — after collecting 3 coins, Mario grows (taller
  lower body). Shrinks back to Small on death.

### Pickups
- **5 coins** scattered across the level above platforms. Gold 16×16
  rects. Disappear (off-screen) on pickup.
- **Coin counter** increments per pickup.

### End conditions
- **Flag touch** → win flag raised (green banner at top of screen).
- **Mario exits** at frame ≥ 240 ("mario done"). Lives/won state shown
  in HUD; gameplay-driven exit kept in roadmap as future work.

### HUD
- **3 lives indicator** — red squares top-left; turn grey when lost.
- **5 coin counter** — gold mini-squares; lit proportionally.
- **Win banner** — green rect at top-center, off-screen until won.

### Runtime fix
- **Z3 sentinel filter** — effect_dispatch suppresses Print/Println
  outputs matching `!N!` patterns (Z3 model-extracted auto-named
  strings that leaked through as bogus output).

## Phases not delivered

- **Phase E — mushroom/flower powerup pickups**: skipped; coin count
  drives form change instead.
- **Phase H — multiple distinct levels**: single level only.
- **Phase J — fire flower projectiles**: not attempted (would need a
  projectile entity system + new physics).
- **Phase K — animation polish**: walking-cycle color shift, squash on
  landing, death flash — not added.

## Performance

Mario with FZ enabled, function-izer + slow-path cache:
- Display: ~14-15 ms/tick (gap-fill refused due to free `win.renderer`,
  falls to cached slow path).
- Keyboard: ~0.3 ms/tick (JIT-compiled).
- Game: ~3-5 ms/tick (now with much more game logic).
- Total: ~20-25 ms/tick ⇒ ~40-50 fps.

## How to run

```bash
./runtime/target/release/evident effect-run examples/test_21_mario/main.ev
```

Controls: arrow keys (left/right), up to jump.

Visually confirmed via screenshots at /tmp/mario-debug/*.png.
