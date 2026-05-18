# Super Mario Brothers — Long-Range Feature Roadmap

Take Mario from "Mario sprite + 3 platforms + 2 enemies" to a recognizable
Super Mario Bros. game. Implemented autonomously over many sessions.

## Constraints / non-goals

Things we **can** do (existing runtime capability):
- SDL rendering via FFI (color/fill rect, color/fill triangle, line, present, delay).
- Multiple FSMs coordinated via World.
- Per-tick state pinning (`_var` time-shift).
- Frame counter for animation.
- Keyboard input (already wired).

Things we **can't** do today and won't try:
- Audio (no SDL_mixer FFI bindings).
- Bitmap fonts / TTF rendering (no SDL_ttf bindings).
- Asset loading (no image loading; everything stays primitive shapes).
- Networking / multiplayer.

Visual elements use SDL primitive shapes — fill rects of varying color do
all the work. Lives counter is a row of red squares. Coins are golden
rects. Hearts in the HUD are pink squares. No text — we communicate state
through color/shape choices.

## Phase A: Side-scrolling camera + wider level

**Goal**: Mario scrolls within a level that's wider than the screen.

Steps:
1. World gains `camera_x ∈ Int` (left edge of viewport, world coords).
2. Level width: 1920 world units (3 screens wide @ 640).
3. Display projects rects from world → screen: `screen.x = world.x - camera_x`.
4. Camera follows Mario: when Mario crosses 320 (screen midpoint), camera
   shifts right; clamped to [0, level_width - 640].
5. Off-screen rects clipped (don't dispatch SDL fill if outside viewport).

Verify: hold right, Mario walks → camera scrolls → new platforms come
into view.

## Phase B: Mario forms (Small / Big / Fire)

**Goal**: Mario changes form when picking up power-ups; sprite changes
visually (height + color tint).

Steps:
1. `enum MarioForm = Small | Big | Fire` in World.
2. MarioSprite gains a `form` parameter; rects vary by form.
3. Small: 32×32 (current). Big: 32×48 (taller). Fire: 32×48 with red tint
   + white shirt instead of brown.
4. Hitbox follows form (collision logic uses form-aware AABB).

Verify: visual form change when power-up picked up (Phase E).

## Phase C: Lives + Death + Respawn

**Goal**: Mario can die. Death decrements lives. Game over at 0.

Steps:
1. World gains `lives ∈ Nat` (starts at 3) and `dead ∈ Bool`.
2. Death conditions:
   a. Mario falls below `world_floor_y` (deep pit).
   b. Mario touches Goomba from the side or below.
3. On death: decrement lives, respawn at level start with full state.
4. If lives == 0: Effect::Exit(0) with "GAME OVER" Println.
5. Optional Phase B coupling: Fire/Big Mario takes damage → Small first,
   only die at Small.

Verify: walk Mario into Goomba → see lives decrement, Mario respawn.
Walk Mario off a cliff → same.

## Phase D: Goomba enemies + stomp interaction

**Goal**: Goombas walk back and forth, can be stomped from above.

Steps:
1. Existing enemies already patrol. Add: stomp detection (Mario's bottom
   intersects Goomba's top while falling).
2. On stomp: enemy dies (removed/marked dead in world.enemies state).
3. Mario gets a small bounce (vel.y = -8) on successful stomp.
4. Side/below contact → death (Phase C).

Verify: jump on Goomba → it disappears, Mario bounces. Walk into one →
Mario dies.

## Phase E: Power-ups (Mushroom + Fire Flower)

**Goal**: Pickup boxes give Mario forms.

Steps:
1. Add `powerups ∈ Seq(Powerup)` to world. Each Powerup has pos, kind
   (Mushroom | Flower), and `active ∈ Bool`.
2. Render: mushrooms red/white, flowers orange/yellow.
3. Mario AABB intersects Powerup pos → consume:
   - Mushroom: Small → Big, Big/Fire → score bonus.
   - Flower: Big → Fire, Small → Big (or skip to Fire).
4. Consumed powerup disappears (active = false).

Verify: walk Mario into mushroom → see form change to Big. Then walk
into flower → Fire form.

## Phase F: Coins

**Goal**: Collectible coins, score counter.

Steps:
1. World gains `coins ∈ Nat` (collected count).
2. Add `coins_in_level ∈ Seq(Coin)`. Coin has pos and `collected ∈ Bool`.
3. Render: golden rects (240, 200, 60, 255) ~16×16.
4. Mario intersects coin → increment world.coins, mark collected.
5. HUD: row of mini gold squares showing collected count.

Verify: walk through coins → count goes up, coin disappears.

## Phase G: End-of-level flag + castle

**Goal**: Each level ends with a flag pole; Mario touches → next level.

Steps:
1. Add `flag_x` to level data.
2. Render flag pole (vertical white line) + flag triangle (red).
3. Render castle (grey rectangle with battlements) right of flag.
4. Mario reaches flag → set `world.level_complete = true`.
5. On `level_complete` for 60 ticks (1s): advance to next level OR
   `Exit(0)` with "thank you mario! but our princess is in another castle".

Verify: walk Mario to right edge → see flag + castle. Touch flag → level
advances or game ends.

## Phase H: Multiple levels (1-1, 1-2, ...)

**Goal**: 3 distinct levels with different geometry.

Steps:
1. Level data structure: pick by `world.level_idx`.
2. Each level has its own platforms, enemies, coins, powerups, flag_x.
3. On level complete: reset Mario position, increment level_idx, reload.

Verify: complete level 1 → start in level 2 with different layout.

## Phase I: HUD (lives + coins + level indicator)

**Goal**: Top-of-screen HUD showing game state.

Steps:
1. HUD background bar at y=0..32, semi-transparent dark.
2. Lives: row of red squares (one per life) at top-left.
3. Coins: row of gold squares at top-middle, total = world.coins.
4. Level: colored indicator squares at top-right (one per level reached).
5. Render HUD AFTER all gameplay (so it draws on top).

Verify: HUD updates as Mario collects coins / loses lives / advances levels.

## Phase J: Fire flower projectiles (stretch goal)

**Goal**: Fire Mario can shoot bouncing fireballs.

Steps:
1. Fire form + space pressed → spawn fireball entity.
2. Fireballs: arc trajectory (Int physics), bounce off platforms.
3. Fireballs kill enemies on contact.
4. Limit: 2 active fireballs at a time.

Verify: as Fire Mario, press fire key → see orange ball arc forward,
hit enemy → enemy dies.

## Phase K: Polish + COUNTEREXAMPLES audit

Steps:
1. Animate Mario's walking via `frame % 8` color shift.
2. Squash on landing (vel.y > 6 → wider+shorter for 4 frames).
3. Audit COUNTEREXAMPLES.md for any new gaps surfaced.
4. Final test suite + screenshot showing all features.

## Execution order (with subagent parallelism where possible)

Serial dependencies:
- Phase A (camera) is foundation; everything depends on it.
- Phase B (forms) needs camera; Phase E (powerups) needs B; Phase J needs E.
- Phase C (lives/death) needs D (enemies for collision).
- Phase F (coins), G (flag), I (HUD) are independent of each other after A.

Subagent dispatch (each phase to its own agent where state allows):
- Camera + level refactor: single agent.
- Mario forms + sprite: single agent.
- Goomba stomp: single agent.
- Lives/death/respawn: single agent.
- Powerups: single agent (after forms).
- Coins: single agent.
- Flag + level advance: single agent.
- HUD: single agent (last; renders on top of everything).
- Fire projectiles: optional, single agent.

Between phases:
- Run `./test.sh`.
- Run Mario, take screenshot to verify rendering.
- Read screenshot, confirm visual correctness, then move on.
- Commit each phase separately.

## Success criteria

The final Mario should:
1. Have a level wider than the screen, side-scroll as Mario walks.
2. Have at least 3 enemies that can be stomped or kill Mario on contact.
3. Have at least 2 power-ups (mushroom + flower), Mario form changes visible.
4. Have at least 5 coins per level, collectible, count displayed in HUD.
5. Have lives counter, Mario dies on enemy contact / fall, respawns 3 times.
6. Have end-of-level flag, touching it ends or advances level.
7. Have at least 2 distinct levels with different layouts.
8. Render correctly at ≥ 30 fps (visually smooth).
9. Pass all of `./test.sh` (451 cargo + 119 conformance).
