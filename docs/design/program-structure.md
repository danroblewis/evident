# Structuring Evident Programs

Practical guidance for decomposing a problem into a collection of constraint
systems and composing them into a working program. The examples below are drawn
from `programs/adventure2/`.

---

## The Layered Stack

Good Evident programs follow a consistent layer order:

```
data layer     — ground facts: enumerated constants and lookup tables
type layer     — pure structs and state snapshots (local invariants only)
trait layer    — small reusable behavioral claims
claim layer    — relations, dispatch logic, transition systems
entry point    — wiring only, no logic
```

Each layer depends only on layers below it. The entry point (`type main`) is
five lines and knows nothing about the game.

```
world.ev          — ItemKind enum, room layouts, item data (ground facts)
game_state.ev     — GameState type with safety invariant (type layer)
commands.ev       — Verb enum, ParsedCommand type, vocabulary tables (data + types)
transition.ev     — trait claims + GameTransition with subclaims (trait + claim layers)
adventure2.ev     — ..LineReader, ..LineWriter, ..GameTransition wiring (entry point)
```

The test is: can you add a new room without touching transition.ev? Can you add
a new verb without touching world.ev? If yes, the layers are clean.

---

## Data Layer: Complete Facts

Ground facts are unconditional assertions about the world. They come in two kinds:

**Enumerations** name the set of possible values without assigning meaning:

```evident
type ItemKind = Torch | Key | Sword | GoldCoin | OldMap
type Verb     = Go | Take | Look | Inventory | Help | Quit
```

**Lookup tables** assign meaning to those values:

```evident
assert item_names = {
    (Torch, "torch"), (Key, "key"), (Sword, "sword"), ...
}
assert initial_item_locations = {
    (Torch, "entrance"), (Key, "forest"), ...
}
```

### The Complete Lookup Pattern

**This is the most important constraint on data design.** Partial lookup tables
— tables that only contain entries for valid cases — create a free-variable trap.

Consider: `((location, direction, dest) ∈ exits_map) ⇒ response = room_desc`.
If `exits_map` only contains valid exits, Z3 can satisfy the constraint by
choosing `dest = ""` to make the antecedent false — then fire the wrong branch.
Z3 did exactly this and the game gave wrong output.

The fix: a *complete* lookup table with a sentinel for blocked cases:

```evident
assert direction_exits = {
    ("entrance", "north", "forest"),   -- valid exit
    ("entrance", "south", ""),         -- blocked: empty string sentinel
    ...
}
```

Given a concrete room and direction, `direction_exits` has exactly one matching
entry and `dest` is uniquely determined. The solver cannot choose `dest`
arbitrarily. Then branch on `dest` positively:

```evident
dest ≠ "" ⇒  -- valid exit
    state_next.location = dest
    response = room_desc
dest = "" ⇒  -- blocked
    response = "You can't go that way."
    PreservesLocation
```

**Rule:** whenever you have `(A, B, result) ∈ table` and you need a unique
`result`, make the table complete: every `(A, B)` pair has an entry, with a
sentinel value for the "nothing" case. Then constrain on `result` positively,
not on membership.

---

## Type Layer: Pure Structs

A `type` is a noun with a shape. Its constraints are local invariants on its
own fields, with no external dependencies.

```evident
type GameState
    location  ∈ String
    inventory ∈ Seq(ItemKind)
    turn      ∈ Nat
    location = "dungeon" ⇒ Torch ∈ inventory  -- local invariant only
```

**Pure structs** (no invariants) bundle fields for cleaner APIs:

```evident
type ParsedCommand
    verb     ∈ Verb
    verb_str ∈ String
    argument ∈ String

type Item
    kind ∈ ItemKind
    name ∈ String
```

These reduce multi-variable clusters to single named variables. Instead of
`verb`, `verb_str`, `argument` as three top-level declarations, `parsed ∈ ParsedCommand`
collapses them and documents what they represent as a group.

### The Constraint Scope Rule

**Constraints that reference external data cannot live in a type body.**

When `item ∈ Item` is expanded via sub-schema instantiation, the sub-env
contains only Item's own fields (`kind`, `name`). A constraint like
`(kind, name) ∈ item_names` would fail silently because `item_names` is not in
that sub-env. It belongs in the claim that uses Item, where the global facts are
in scope:

```evident
-- Wrong: item_names not in scope during type expansion
type Item
    kind ∈ ItemKind
    name ∈ String
    (kind, name) ∈ item_names   -- silently dropped!

-- Right: put it in the claim where item_names is visible
subclaim TakeAction
    (item.kind, item.name) ∈ item_names  -- item_names is in scope here
    ...
```

If you find yourself writing a type constraint that references a lookup table,
move that constraint to the claim that uses the type, or make it a separate
`claim`.

---

## Trait Layer: Reusable Behavioral Fragments

Traits are small claims that capture one aspect of a state transition. They are
self-contained, composable, and read like their names:

```evident
claim PreservesInventory
    state ∈ GameState; state_next ∈ GameState
    state_next.inventory = state.inventory

claim AdvancesTurn
    state ∈ GameState; state_next ∈ GameState
    state_next.turn = state.turn + 1
```

A trait is always named as a noun or adjective, not a verb. `PreservesInventory`
not `PreserveInventory`; `AdvancesTurn` not `AdvanceTurn`.

Traits compose via names-match. When a subclaim declares `PreservesInventory`,
the names `state` and `state_next` in scope unify automatically:

```evident
subclaim LookAction
    PreservesInventory   -- names match, no mapping needed
    PreservesLocation
    AdvancesTurn
    ...
```

**Every subclaim fully declares its behavior** across all state dimensions.
Nothing is implicit. If a subclaim doesn't declare `PreservesInventory`, it must
declare what happens to the inventory instead. This makes each subclaim a
complete, auditable transition definition.

---

## Claim Layer: The Dispatch Pattern

A transition claim dispatches to subclaims based on a condition. The `⟸`
operator makes dispatch tables read naturally — "this action applies when this
condition holds":

```evident
claim GameTransition
    state      ∈ GameState
    state_next ∈ GameState
    cmd        ∈ String
    response   ∈ String
    ready      ∈ Bool
    parsed     ∈ ParsedCommand
    item       ∈ Item

    subclaim LookAction     -- defines what "look" does
        ...

    subclaim GoAction       -- defines what "go" does
        ...

    ready ⇒
        (parsed.verb_str, parsed.verb) ∈ verb_words
        LookAction ⟸ parsed.verb = Look
        GoAction   ⟸ parsed.verb = Go
        ...
```

`A ⟸ B` means `B ⇒ A`. Reading down the dispatch table: "LookAction applies
when verb is Look. GoAction applies when verb is Go." Only the matching branch
fires; all others are vacuously satisfied.

### Variable Scope Planning

The parent claim declares the **interface** — variables visible to all subclaims
and to the caller. Subclaims declare **implementation details** that aren't
needed outside.

**Rule of thumb:** a variable belongs at the parent level if more than one
subclaim uses it. A variable belongs inside a subclaim if only that subclaim
needs it.

```
Parent (interface):
    state, state_next — shared by all subclaims
    parsed            — verb, verb_str, argument used by all branches
    item              — used by TakeAction, kept parent for sub-schema expansion

Subclaim-internal (implementation):
    direction, dest, room_desc   — only GoAction needs these
    room_desc (separate)         — only LookAction needs this
```

Internal variables declared inside a subclaim get fresh Z3 constants and are
not visible to the parent or to other subclaims. This scoping is enforced by the
runtime, not just a convention.

### The Idle/Active Guard Pattern

When a transition claim is used with an executor, it typically guards its logic
with a `ready ∈ Bool` variable:

```evident
ready ⇒
    -- active step: parse and dispatch
    ...
¬ready ⇒
    -- idle step: preserve state (or initialize on turn 0)
    response = ""
    state_next.turn = state.turn
    state.turn = 0 ⇒
        state_next.location = "entrance"
        #state_next.inventory = 0
    state.turn > 0 ⇒
        state_next.location = state.location
        state_next.inventory = state.inventory
```

The idle branch must also fully specify `state_next` to avoid free variables.
Turn 0 is initialization; later turns preserve. This pattern ensures the solver
always has a unique solution on idle steps.

---

## Entry Point: Wiring Only

The entry point (`type main`) should contain no logic. It wires I/O traits to
the transition claim via passthrough composition:

```evident
type main
    ..LineReader
    ..LineWriter
    state      ∈ GameState
    state_next ∈ GameState
    ..GameTransition (cmd mapsto line, response mapsto line_out, ready mapsto line_ready)
```

Variable naming is the API. `cmd` in GameTransition maps to `line` from LineReader.
The rename documents the interface contract. If the names matched, no rename
would be needed.

The test: if main has any constraint beyond variable declarations and passthroughs,
something belongs lower in the stack.

---

## Worked Example: Reduction from Flat to Structured

The first version of `GameTransition` had 14 top-level variable declarations and
all logic inline. The final version has 7 declarations and named subclaims.

**Before:**
```
claim GameTransition
    state, state_next ∈ GameState
    cmd, response ∈ String
    ready ∈ Bool
    verb ∈ Verb; verb_str, argument ∈ String   -- 3 parsing vars
    direction, dest, room_desc ∈ String         -- 3 movement vars
    item ∈ Item; item_name ∈ String             -- 2 item vars
    ...14 items, all at the same level...
```

**After:**
```
claim GameTransition
    state, state_next ∈ GameState
    cmd, response ∈ String
    ready ∈ Bool
    parsed ∈ ParsedCommand    -- replaces 3 parsing vars
    item   ∈ Item             -- replaces item + item_name (item.kind, item.name)
    -- direction/dest/room_desc moved into subclaims
```

The reduction steps:

1. **Identify clusters.** The 14 vars naturally grouped: parsing (verb/verb_str/argument),
   movement (direction/dest/room_desc), item (item/item_name). Clusters become types.

2. **Promote to types.** `ParsedCommand` captures the parsing cluster. `Item` captures
   the item cluster (renaming the enum to `ItemKind` to free the name).

3. **Scope the implementation details.** Movement vars are only used by one subclaim.
   Move them inside `GoAction` as internal vars. Same for `room_desc` in `LookAction`.

4. **Name the branches.** Inline dispatch blocks become subclaims. `verb = Look ⇒ {...}`
   becomes `subclaim LookAction { ... }` and `LookAction ⟸ parsed.verb = Look`.

5. **Extract traits.** Repeated patterns (`state_next.inventory = state.inventory`,
   `state_next.turn = state.turn + 1`) become reusable trait claims.

Each step reduces the problem scope visible at any one point in the code.
The resulting program is easier to audit: each subclaim is a complete, isolated
transition definition that can be read independently.

---

## Common Refactors

When the same constraint shape repeats with only small differences, the right
move is almost always a parameterized claim invoked with `mapsto` mappings.
Below are the patterns that come up most often, with examples drawn from
`programs/sdl_demo/collect.ev`.

### Repeated structural assignments → parameterized claim

If you find yourself writing the same field-by-field assignment four times
with only positions changing, that's a claim. The four dot-rendering blocks:

```evident
-- Before: 4 × 2 lines, only position differs
¬state.d0 ⇒ (dot0_rect.x = 80  ∧ dot0_rect.y = 80  ∧ dot0_rect.w = 25 ∧ ... )
state.d0  ⇒ (dot0_rect.w = 0   ∧ dot0_rect.h = 0)
¬state.d1 ⇒ (dot1_rect.x = 660 ∧ dot1_rect.y = 80  ∧ ... )
...
```

Becomes one definition + four invocations:

```evident
subclaim Dot
    rect      ∈ SDLRect
    collected ∈ Bool
    pos_x     ∈ Int
    pos_y     ∈ Int

    ¬collected ⇒ (rect.x = pos_x ∧ rect.y = pos_y ∧ rect.w = 25 ∧ ... )
    collected  ⇒ (rect.w = 0 ∧ rect.h = 0)

Dot (rect mapsto dot0_rect, collected mapsto state.d0, pos_x mapsto 80,  pos_y mapsto 80)
Dot (rect mapsto dot1_rect, collected mapsto state.d1, pos_x mapsto 660, pos_y mapsto 80)
Dot (rect mapsto dot2_rect, collected mapsto state.d2, pos_x mapsto 80,  pos_y mapsto 460)
Dot (rect mapsto dot3_rect, collected mapsto state.d3, pos_x mapsto 660, pos_y mapsto 460)
```

Mappings can be **literals** (`pos_x mapsto 80`), **field accesses**
(`player_x mapsto state.player_x`), or any expression — not just bare
identifiers. The `mapsto` value gets translated as an expression in the
caller's scope and bound to the parameter.

### Mirrored axes / dimensions → axis-parameterized claim

X and Y physics are usually structurally identical with axis-specific
inputs. Extract one `AxisPhysics` claim, invoke twice:

```evident
subclaim AxisPhysics
    pos       ∈ Int
    pos_next  ∈ Int
    v         ∈ Int
    v_next    ∈ Int
    pos_min   ∈ Int
    pos_max   ∈ Int
    won       ∈ Bool
    accel_pos ∈ Bool        -- right or down key
    accel_neg ∈ Bool        -- left or up key
    intended  ∈ Int          -- subclaim-internal: fresh per composition

    -- acceleration / deceleration / wall clamping
    ...

AxisPhysics (pos mapsto state.player_x, ... accel_pos mapsto input.right_held, accel_neg mapsto input.left_held)
AxisPhysics (pos mapsto state.player_y, ... accel_pos mapsto input.down_held,  accel_neg mapsto input.up_held)
```

Each composition gets its own fresh `intended` (subclaim-internal vars
are fresh Z3 constants per invocation).

### Active/inactive toggle → "show full state when active, minimal when not"

Anywhere you have something that's optionally present (a dot that may be
collected, an enemy that may be defeated, a UI element that may be
hidden), the pattern is the same:

```evident
¬state.active ⇒ (full set of constraints describing the visible state)
state.active  ⇒ (minimal "invisible" state — w=0, fd=-1, opacity=0, etc.)
```

The invisible state is whatever the consumer (renderer, executor) treats
as "skip me." For SDL rects it's `w=0 ∧ h=0`. For an inventory slot it
might be `kind = Empty`. Pick a sentinel that's safe to ignore.

### Repeated antecedents → trait bundle

Two or more separate constraints sharing the same antecedent is a hint that
the antecedent should fire one trait that does both:

```evident
-- Before
state.won ⇒ output.bg.r = 20
state.won ⇒ output.bg.g = 120
state.won ⇒ output.bg.b = 50

-- After
state.won ⇒ (output.bg.r = 20 ∧ output.bg.g = 120 ∧ output.bg.b = 50)
```

If the antecedent fires across many constraints (`PreservesInventory`,
`PreservesLocation`, `AdvancesTurn` all gated by `verb = Look`), bundle
them into a trait claim and dispatch once.

### Long disjoint `⇒` chains → subclaim + `⟸` dispatch table

When you have many `condition ⇒ (lots of constraints)` blocks where the
conditions are mutually exclusive, pull each consequent into a named
subclaim and dispatch:

```evident
-- Before
verb = Look ⇒ ( ...look constraints... )
verb = Go   ⇒ ( ...go constraints... )
verb = Take ⇒ ( ...take constraints... )

-- After
subclaim LookAction { ... }
subclaim GoAction   { ... }
subclaim TakeAction { ... }

LookAction ⟸ verb = Look
GoAction   ⟸ verb = Go
TakeAction ⟸ verb = Take
```

The named subclaim is its own self-contained transition; the dispatch
table reads as a verb-to-action map.

### Decision guide

| Smell | Refactor |
|---|---|
| 4× similar blocks differ only in position/colour/etc. | Parameterized claim, `mapsto` mappings |
| X and Y axes (or N rows/columns) with identical structure | Axis-parameterized claim, called per axis |
| `condition ⇒ A; condition ⇒ B; condition ⇒ C` (same antecedent) | Combine: `condition ⇒ (A ∧ B ∧ C)` or trait |
| Optional rendering / appearance | Active/inactive toggle with sentinel "invisible" state |
| Many disjoint `⇒` branches with nameable consequents | Subclaims + `⟸` dispatch |
| Repeated trait combos across subclaims | Bundle traits into one claim (`StateTurn`, `MetaTurn`) |

---

## Diagnostic Questions

When a constraint model feels tangled:

- **Are all lookup tables complete?** Any partial table is a source of Z3 non-determinism.
  Add entries for all blocked cases with a sentinel value.

- **Do any types reference external data?** Constraints involving lookup tables
  belong in a claim at the scope where those tables are visible, not in a type body.

- **Are there variable clusters that always appear together?** They may be a type.
  If two variables always co-appear in the same constraints, name the pair.

- **Are there repeated constraint patterns across branches?** They may be a trait.
  If three subclaims all say `state_next.inventory = state.inventory`, extract `PreservesInventory`.

- **Can you name each dispatch branch?** If a branch of `verb = X ⇒ { ... }` is
  too complex to summarize in a name, it may need further decomposition. If it
  fits in a subclaim name, the name becomes the documentation.

- **Does the parent declare its own implementation details?** Variables only used
  inside one subclaim should be internal to that subclaim. The parent's variable
  list should be its public interface, not its full state.
