# Text Adventure Game — Rewritten for Modern Evident

The original plan (2024) was written before the streaming executor, implies
blocks, sequences, string operations, and the I/O trait library existed. This
rewrite uses the full current language.

A text adventure is the ideal showcase program for Evident because it requires
every major feature in one place: relational world modelling, string parsing
(bidirectional), state machine execution, response generation, and constraint-
based validation.

---

## What We Have Now (That Changes Everything)

| Feature | What it enables |
|---|---|
| `schema main` with `..LineReader` / `..LineWriter` | The game loop, no Python glue |
| State variables carried by executor | Player location, inventory, turn |
| `raw = "go " ++ dir_word` bidirectional | Command parsing as constraint solving |
| `∈` on `Seq(Item)` | Inventory membership checks |
| Implies blocks `verb = Go ⇒ \n   ...` | Clean state transitions |
| `bool ⇒` and `¬bool ⇒` | Conditional responses |
| Named sets + tuple relations | World graph, verb map, descriptions |
| `++` string concatenation | Response text construction |
| `∋` contains | Checking keywords in player input |
| Import system | Split across files cleanly |

---

## File Structure

```
programs/adventure/
  world.ev          -- rooms, items, exits, descriptions, item locations
  commands.ev       -- verb/direction/item enums and string→enum relations
  transitions.ev    -- GameTransition: (state, command) → (next_state, response)
  adventure.ev      -- schema main: executor wiring
```

---

## World Model (`world.ev`)

```evident
type Room      = Entrance | Forest | Cave | Tower | Dungeon
type Item      = Torch | Key | Sword | GoldCoin | OldMap
type Direction = North | South | East | West | Up | Down

-- The map: (from_room, direction, to_room)
assert exits = {
    (Entrance, North, Forest),
    (Entrance, East,  Cave),
    (Forest,   South, Entrance),
    (Forest,   East,  Tower),
    (Cave,     West,  Entrance),
    (Cave,     Down,  Dungeon),
    (Tower,    West,  Forest),
    (Dungeon,  Up,    Cave)
}

-- Starting positions of items (mutable — changes as player picks up items)
-- These are the *initial* positions; runtime state tracks current positions.
assert initial_item_locations = {
    (Torch,    Entrance),
    (Key,      Forest),
    (Sword,    Cave),
    (GoldCoin, Tower),
    (OldMap,   Dungeon)
}

-- Room descriptions
assert room_descriptions = {
    (Entrance, "You stand at the entrance to a dark forest. Paths lead north and east."),
    (Forest,   "A dense forest. You can go south or east. Something glints nearby."),
    (Cave,     "A damp cave. Passages lead west and down into the dark."),
    (Tower,    "A ruined tower with a sweeping view. The only exit is west."),
    (Dungeon,  "A cold stone dungeon. A ladder leads up.")
}
```

---

## Command Parsing (`commands.ev`)

The key insight: command parsing is a constraint. Given `raw = "go north"`,
the solver finds `verb = Go` and `argument = "north"` by solving
`raw = verb_word ++ " " ++ argument` against the verb/direction maps.
This is fully bidirectional — no parser code needed.

```evident
import "world.ev"

type Verb = Go | Take | Drop | Look | Inventory | Help | Quit

-- String → Verb relation
assert verb_words = {
    ("go",        Go),
    ("walk",      Go),
    ("move",      Go),
    ("take",      Take),
    ("get",       Take),
    ("pick",      Take),
    ("drop",      Drop),
    ("leave",     Drop),
    ("look",      Look),
    ("examine",   Look),
    ("l",         Look),
    ("inventory", Inventory),
    ("inv",       Inventory),
    ("i",         Inventory),
    ("help",      Help),
    ("quit",      Quit),
    ("q",         Quit)
}

-- String → Direction relation
assert direction_words = {
    ("north", North), ("n", North),
    ("south", South), ("s", South),
    ("east",  East),  ("e", East),
    ("west",  West),  ("w", West),
    ("up",    Up),    ("u", Up),
    ("down",  Down),  ("d", Down)
}

-- String → Item relation
assert item_words = {
    ("torch",    Torch),
    ("key",      Key),
    ("sword",    Sword),
    ("gold",     GoldCoin),
    ("coin",     GoldCoin),
    ("map",      OldMap),
    ("old map",  OldMap)
}

-- A parsed command: the solver finds verb + argument from the raw string
schema ParsedCommand
    raw      ∈ String
    verb     ∈ Verb
    verb_str ∈ String
    argument ∈ String

    -- Either: "verb argument" (two-word command)
    -- Or:     "verb"         (single-word command)
    (raw = verb_str ++ " " ++ argument) ∨ (raw = verb_str ∧ argument = "")

    -- The verb string maps to the verb enum
    (verb_str, verb) ∈ verb_words
```

---

## Game State

```evident
import "world.ev"

schema GameState
    location  ∈ Room
    inventory ∈ Seq(Item)
    turn      ∈ Nat

    -- Safety constraint: Dungeon requires Torch in inventory
    location = Dungeon ⇒ Torch ∈ inventory
```

---

## Transitions (`transitions.ev`)

State transitions are implications. The schema takes a current state and
a parsed command and produces the next state plus a response string.

```evident
import "world.ev"
import "commands.ev"

schema GameTransition
    state      ∈ GameState
    cmd        ∈ ParsedCommand
    next       ∈ GameState
    response   ∈ String

    -- ── GO ────────────────────────────────────────────────────────────────
    cmd.verb = Go ⇒
        direction ∈ Direction
        (cmd.argument, direction) ∈ direction_words
        -- Valid exit: move there
        (∃ dest ∈ Room : (state.location, direction, dest) ∈ exits) ⇒
            (state.location, direction, next.location) ∈ exits
            next.inventory  = state.inventory
            next.turn       = state.turn + 1
            -- Response: room description
            (next.location, room_desc) ∈ room_descriptions
            response = room_desc
        -- No exit in that direction
        (¬∃ dest ∈ Room : (state.location, direction, dest) ∈ exits) ⇒
            next.location   = state.location
            next.inventory  = state.inventory
            next.turn       = state.turn + 1
            response        = "You can't go that way."

    -- ── LOOK ──────────────────────────────────────────────────────────────
    cmd.verb = Look ⇒
        next.location   = state.location
        next.inventory  = state.inventory
        next.turn       = state.turn + 1
        (state.location, room_desc) ∈ room_descriptions
        response = room_desc

    -- ── TAKE ──────────────────────────────────────────────────────────────
    cmd.verb = Take ⇒
        item ∈ Item
        (cmd.argument, item) ∈ item_words
        -- Item is present and not already held
        (item, state.location) ∈ current_item_locations ∧ item ∉ state.inventory ⇒
            next.location  = state.location
            next.inventory = state.inventory ++ ⟨item⟩
            next.turn      = state.turn + 1
            (item, item_name) ∈ item_names
            response = "You pick up the " ++ item_name ++ "."
        -- Item not here
        ¬((item, state.location) ∈ current_item_locations ∧ item ∉ state.inventory) ⇒
            next.location  = state.location
            next.inventory = state.inventory
            next.turn      = state.turn + 1
            response       = "You don't see that here."

    -- ── DROP ──────────────────────────────────────────────────────────────
    cmd.verb = Drop ⇒
        item ∈ Item
        (cmd.argument, item) ∈ item_words
        item ∈ state.inventory ⇒
            next.location  = state.location
            next.turn      = state.turn + 1
            (item, item_name) ∈ item_names
            response = "You drop the " ++ item_name ++ "."
        item ∉ state.inventory ⇒
            next.location  = state.location
            next.inventory = state.inventory
            next.turn      = state.turn + 1
            response       = "You're not carrying that."

    -- ── INVENTORY ─────────────────────────────────────────────────────────
    cmd.verb = Inventory ⇒
        next.location   = state.location
        next.inventory  = state.inventory
        next.turn       = state.turn + 1
        #state.inventory = 0 ⇒ response = "You are carrying nothing."
        #state.inventory > 0 ⇒ response = "You are carrying some items."

    -- ── HELP / QUIT ───────────────────────────────────────────────────────
    cmd.verb = Help ⇒
        next = state
        response = "Commands: go <dir>, take <item>, drop <item>, look, inventory, quit"

    cmd.verb = Quit ⇒
        next = state
        response = "Goodbye."
```

---

## Main Program (`adventure.ev`)

```evident
import "stdlib/line-reader.ev"
import "stdlib/line-writer.ev"
import "world.ev"
import "commands.ev"
import "transitions.ev"

-- Initial state
schema InitialState
    location  = Entrance
    inventory = ⟨⟩
    turn      = 0

schema main
    ..LineReader                   -- reads player commands line by line
    ..LineWriter                   -- writes responses
    state      ∈ GameState
    state_next ∈ GameState
    cmd        ∈ ParsedCommand
    transition ∈ GameTransition

    -- Wire parsed command to input line
    cmd.raw = line

    -- Wire transition to state and command
    transition.state = state
    transition.cmd   = cmd

    line_ready ⇒
        line_out   = transition.response
        state_next = transition.next

    ¬line_ready ⇒
        line_out        = ""
        state_next.location  = state.location
        state_next.inventory = state.inventory
        state_next.turn      = state.turn
```

---

## What Works Today vs What Needs Minor Additions

### Works today, implement as-is:

- World model: rooms, exits, items, directions — named sets and tuple relations
- Verb mapping via `(verb_string, Verb) ∈ verb_words`
- Direction/item mapping the same way
- `GameState` schema with enum and Seq constraints
- Movement with `∈ exits` tuple lookup
- Look command
- Dungeon safety constraint (`location = Dungeon ⇒ Torch ∈ inventory`)
- The streaming executor loop via `schema main` with `..LineReader`/`..LineWriter`
- Implies blocks for clean state transition logic

### Needs small additions:

- **`current_item_locations`**: item positions change as the player picks up/drops
  items. The initial positions are a named set, but the runtime positions need
  to be part of the game state. Solution: `GameState` carries `items_in_room ∈ Seq(Item)`,
  or we track picked-up items and derive room contents from the difference.

- **Inventory display**: `#inventory > 0 ⇒ response = "You carry: ..."` requires
  formatting a `Seq(Item)` as a comma-separated string. This needs either a
  stdlib helper (`format_list`) or `str_to_int` / `adjacent` patterns for
  sequential string building.

- **Command parsing for ambiguous verbs**: "pick up torch" vs "pick torch" —
  the `verb_str ++ " " ++ argument` split needs `or` for alternate prefixes.
  Achievable with `∨` chains.

### Deferred (needs future language work):

- Substring extraction from arbitrary positions (needs `index_of`/`sub_str`)
- Proper item-in-room tracking as mutable state (needs `Set(T)` type or
  richer `Seq` operations)

---

## Testing Strategy

Using the framework from `testing-framework.md`:

```evident
schema main
    -- @test input="look\n" output contains "entrance"
    -- @test input="go north\n" output contains "forest"
    -- @test input="go south\ngo north\n" state.location = Forest
    -- @property satisfies GameState        -- all states are valid
    -- @property state.turn increases       -- turn always advances
    -- @test input="go down\n" (without torch) output="You can't go that way."
```

The game's constraint system IS the validator. Feed the program's output
back into `NumberedDocument`-style schemas to verify correctness without
hardcoded expected strings.

---

## Implementation Order for the Subagent

1. `world.ev` — rooms, exits, items, descriptions, item/direction name maps
2. `commands.ev` — verb/direction/item enums and string→enum relations
3. `adventure.ev` — schema main with LineReader/LineWriter, basic loop
4. Verify: `echo "look" | evident execute adventure.ev` shows room description
5. Add movement: `go north`, `go south` etc.
6. Add item interaction: `take` and `drop`
7. Add inventory display
8. Add the Dungeon safety constraint (can't enter without Torch)
9. Add win condition (reach Tower with all items?)
10. Write `-- @test` annotations for key scenarios
