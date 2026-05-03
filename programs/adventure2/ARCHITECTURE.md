# adventure2 Architecture

A text adventure game written to demonstrate Evident's type/claim composition
conventions. The design argument is recorded here so the tradeoffs are
explicit rather than implicit in the code.

---

## Keyword choices

Every declaration in this program is either `type` (a noun with a shape) or
`claim` (a relation or property across multiple values).

| Declaration | Keyword | Reason |
|---|---|---|
| `GameState` | `type` | A noun: a snapshot of player location, inventory, and turn counter. The safety invariant (`dungeon ⇒ torch`) is a local constraint on its own fields — the canonical case for a type-level invariant. |
| `GameTransition` | `claim` | A relation: `(state, cmd) → (state_next, response)`. It relates multiple distinct values across a step boundary. It has no single structural subject. |
| `main` | `type` | An entry point that wires I/O traits to the transition claim. It has a shape (the set of variables it declares) but no game logic. |

---

## Composition chain

```
..LineReader                         line, line_ready, partial accumulation
state ∈ GameState                    location, inventory, turn
state_next ∈ GameState               same for next step
..GameTransition (                   all transition constraints, with renames:
    cmd      mapsto line               raw command = accumulated line
    response mapsto line_out           response = LineWriter output
    ready    mapsto line_ready         fires only on complete input lines
)
..LineWriter                         line_out, dst
```

`main` is five lines. All game logic lives in `transition.ev`.

### Why `..GameTransition` at the top level works

The passthrough is unconditional — `GameTransition`'s constraints always apply.
This is fine because `GameTransition` gates its own logic on `ready ∈ Bool`:

- `ready ⇒` — transition fires, parsing and verb dispatch run
- `¬ready ⇒` — response is `""`, state is preserved (or initialised on turn 0)

When `ready = False` (accumulating input characters), the parsing constraint
`(verb_str, verb) ∈ verb_words` does not fire, so it does not cause UNSAT.

### Why `ParsedCommand` was removed

`ParsedCommand` was planned as a separately-composable parsing claim, intended
for `..ParsedCommand (raw mapsto line)` inside a `ready ⇒` block. That
composition pattern does not work in the current runtime — passhthroughs inside
implies blocks are not supported. The parsing lives inline in
`GameTransition`'s `ready ⇒` body instead.

Writing `ParsedCommand` before verifying the composition was possible was the
mistake. Once inline was the only option, `ParsedCommand` became dead code and
was deleted.

---

## Movement: one block instead of six

The original adventure had six direction blocks, each repeating the same
`exits_map → response → state_next.location` pattern. adventure2 reduces this
to two direction-pin implies plus one shared movement block.

### The free-variable problem

With `((state.location, dir, dest) ∈ exits_map) ⇒ body`, `dest` is a free
variable in the implies antecedent. Z3 can satisfy this by choosing
`dest = ""` to make the antecedent vacuously false, then firing the blocked
branch. This produces the wrong output.

### The fix: complete lookup + concrete direction

`direction_exits` maps every `(room, direction)` pair to a destination or
`""`. It is the single authoritative source. Given a concrete room and
direction, Z3 has exactly one satisfying value for `dest`.

Direction is pinned deterministically by two implies with disjoint antecedents:

```evident
argument ≠ "" ⇒ (argument, direction) ∈ direction_words   -- "go north"
argument = "" ⇒ (verb_str, direction) ∈ direction_words   -- "n" alone
```

When `argument = "north"`, the first fires (second's antecedent is false).
When `argument = ""` and `verb_str = "n"`, the second fires. Exactly one
value of `direction` results. Then `direction_exits` gives a unique `dest`.

### Why exits_map was removed from world.ev

`exits_map` (partial, valid exits only) was a strict subset of `direction_exits`
(complete, with `""` for blocked directions). It added no information. Test
claims that previously used `exits_map` were rewritten against `direction_exits`
by filtering `dest ≠ ""`.

---

## The safety invariant is in GameState, not a separate claim

```evident
type GameState
    location  ∈ String
    inventory ∈ Seq(Item)
    turn      ∈ Nat
    location = "dungeon" ⇒ Torch ∈ inventory
```

This is a local invariant on `GameState`'s own fields — no external data. It
belongs in the type. The consequence: any `GameTransition` step that would
produce `state_next.location = "dungeon"` with no Torch in `state_next.inventory`
is UNSAT. The executor silently skips such steps (the dungeon is unreachable
without the torch, without any explicit blocking code in the transition).

If the invariant involved external data (e.g., checking a game configuration
table), it would move to a `claim`.

---

## What is still imperfect

**Unknown commands silently do nothing.** There is no `Unknown` verb. When
the user types an unrecognised command, `(verb_str, verb) ∈ verb_words` has
no solution → UNSAT → executor skips silently. A `¬∃` quantifier over the Verb
enum would let us detect unknown commands and produce "I don't understand", but
Z3's handling of `∃` over algebraic types combined with named set membership
is unreliable in the current runtime.

**Intermediate variables are top-level.** `direction`, `dest`, `room_desc`,
`item`, `item_name` are declared at the top of `GameTransition` even though
they are only meaningful inside specific verb branches. Evident requires all
body variables to be declared at the schema level, so these are unavoidably
global. On `¬ready` steps they are unconstrained free variables — wasteful
but harmless.

**Inventory cannot be displayed as a list.** `#state.inventory > 0 ⇒ response
= "You are carrying some items."` is the best we can do. Formatting a
`Seq(Item)` as a comma-separated string requires string operations on enum
values that Evident does not yet support.
