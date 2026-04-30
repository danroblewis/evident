# Text Adventure Game — Evident Design Plan

A text adventure is the ideal first program that exercises Evident's string
constraint model. It requires:
- Parsing player input (string → structured command)
- World state as a constraint system (rooms, items, exits)
- State transitions (move, take, drop, look)
- Response generation (structured state → descriptive text)

All of these fit naturally into Evident's model without requiring classical
string operations.

---

## The World Model

The world is a set of facts — rooms, items, connections between rooms.

```
type Room = Entrance | Forest | Cave | Tower | Dungeon
type Item = Torch | Key | Sword | GoldCoin | OldMap
type Direction = North | South | East | West | Up | Down

-- Exits: (from_room, direction, to_room)
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

-- Items initially in rooms
assert item_locations = {
    (Torch,    Entrance),
    (Key,      Forest),
    (Sword,    Cave),
    (GoldCoin, Tower),
    (OldMap,   Dungeon)
}

-- Room descriptions (string constraints once string theory is exposed)
assert room_descriptions = {
    (Entrance, "You stand at the entrance to a dark forest. Paths lead north and east."),
    (Forest,   "A dense forest surrounds you. You can go south or east."),
    (Cave,     "A damp cave. Something glints in the darkness. Passages west and down."),
    (Tower,    "A ruined tower. You can see for miles. The only way out is west."),
    (Dungeon,  "A cold stone dungeon. Your torch flickers. There is a ladder up.")
}
```

---

## Game State

The player's current state is a schema:

```
schema GameState
    location  ∈ Room
    inventory ∈ Set Item
    turn      ∈ Nat          -- for time-based events, light levels, etc.
    torch_lit ∈ Bool

    -- Constraints
    turn ≥ 0
    -- Can only be in Dungeon if carrying torch (it's dark)
    location = Dungeon ⇒ Torch ∈ inventory
```

---

## Command Parsing

Player input is a string. Parsing it is a constraint problem.

```
type Verb = Go | Take | Drop | Look | Use | Inventory | Help | Quit

schema ParsedCommand
    raw       ∈ String         -- what the player typed
    verb      ∈ Verb
    argument  ∈ String         -- "north", "torch", etc.

    -- Grammar: "verb argument" or just "verb"
    -- NEEDS: Z3 string constraints (++ and starts_with)
    -- raw = verb_word ++ " " ++ argument  OR  raw = verb_word

    -- Verb word mapping (tuple relation — already works today)
    -- (verb_word, verb) ∈ verb_map where:
```

```
assert verb_map = {
    ("go",        Go),
    ("walk",      Go),
    ("move",      Go),
    ("take",      Take),
    ("pick",      Take),
    ("grab",      Take),
    ("drop",      Drop),
    ("leave",     Drop),
    ("look",      Look),
    ("examine",   Look),
    ("l",         Look),
    ("inventory", Inventory),
    ("i",         Inventory),
    ("use",       Use),
    ("help",      Help),
    ("quit",      Quit),
    ("q",         Quit)
}
```

---

## State Transitions

Each command produces a new game state. Transitions are implications.

```
claim valid_move
    state   ∈ GameState
    command ∈ ParsedCommand
    next    ∈ GameState

    command.verb = Go
    direction ∈ Direction
    -- direction from argument string  (NEEDS: string→enum lookup)
    (state.location, direction, next.location) ∈ exits
    next.inventory = state.inventory   -- inventory unchanged
    next.turn = state.turn + 1

claim valid_take
    state   ∈ GameState
    command ∈ ParsedCommand
    next    ∈ GameState
    item    ∈ Item

    command.verb = Take
    -- item from argument string (NEEDS: string→enum lookup)
    (item, state.location) ∈ item_locations  -- item is here
    item ∉ state.inventory                   -- don't already have it
    next.inventory = state.inventory ∪ {item}
    next.location = state.location
    next.turn = state.turn + 1

claim valid_drop
    state   ∈ GameState
    command ∈ ParsedCommand
    next    ∈ GameState
    item    ∈ Item

    command.verb = Drop
    item ∈ state.inventory
    next.inventory = state.inventory \ {item}
    next.location = state.location
    next.turn = state.turn + 1

claim failed_move
    -- No valid exit in that direction
    state   ∈ GameState
    command ∈ ParsedCommand
    command.verb = Go
    direction ∈ Direction
    ¬∃ dest ∈ Room : (state.location, direction, dest) ∈ exits
```

---

## Response Generation

The response text describes what happened. It is a constraint on a string
given the game state and what changed.

```
schema Response
    command ∈ ParsedCommand
    state   ∈ GameState
    text    ∈ String

    -- Move response: describe new location
    command.verb = Go ⇒
        (state.location, room_desc) ∈ room_descriptions ∧
        text = room_desc   -- simplified; real version would add visible items

    -- Take response
    command.verb = Take ⇒
        text = "You pick up the " ++ item_name ++ "."  -- NEEDS: string ops

    -- Failed move
    -- failed_move state command ⇒ text = "You can't go that way."
```

---

## The Run Loop

With `evident run` handling stdin/stdout:

```
-- adventure.ev

import "world.ev"     -- rooms, items, exits, descriptions
import "commands.ev"  -- verb_map, ParsedCommand
import "transitions.ev"  -- valid_move, valid_take, etc.

-- Initial state assertion
assert start_location = Entrance
assert start_inventory = {}
assert start_turn = 0

-- The runtime reads a line from stdin and binds it as:
-- assert player_input = "go north"
-- Then re-evaluates ? queries

? ParsedCommand raw=player_input          -- parse the input
? valid_move state=current_state command=parsed_command  -- apply it
? Response command=parsed_command state=next_state       -- generate text

-- The ? Response result goes to stdout
-- The runtime updates current_state = next_state for the next turn
```

---

## What Works Today vs. What Needs String Theory

**Works today:**
- Room/item/exit declarations as named sets and tuple relations
- Verb mapping via tuple relation: `(verb_word_string, Verb_enum) ∈ verb_map`
- Game state schema with enum and set constraints
- Transition claims (valid_move, valid_take, etc.) using set operations
- The world structure is entirely expressible

**Needs Z3 string theory (next thing to build):**
- Parsing `"go north"` into verb="go" and argument="north" via string constraints
- Mapping argument strings to enum values (can be done with tuple relations if
  we can split the string first)
- Response text generation via string concatenation/templating

**Needs stdin integration:**
- The run loop: read line from stdin, assert as player_input, evaluate queries
- This is a small runtime change, not a language change

---

## Implementation Order

1. Expose Z3 string theory in Evident grammar and translator (PrefixOf,
   Contains, Concat `++`, Length `|s|`, InRe for regex)

2. Build the world model and transition logic — works today, no new features

3. Wire stdin to the `evident run` loop — runtime change

4. Build the text adventure using string constraints for parsing and response
   generation

5. Use it as the test case for everything: if you can build a text adventure
   in pure Evident with only minimal Python glue, the string and I/O models
   are working.
