# Which variables a portrait is drawn over — the interface, and witnesses

> A visualization of a model is a projection of its state onto a few axes. This
> note records *which* variables those axes default to, and why. Companion to
> [`visualization-survey.md`](./visualization-survey.md) and
> [`phase-portraits-research.md`](./phase-portraits-research.md) (§3.3, "projections
> lie").

## The decision

**The axes default to the model's interface — the claim's first-line variables.**
Internals (body-declared helpers) are not drawn; they are projected out. There is
no new language concept and no viz-config: the interface line a model already has
*is* the declaration of "what is observable."

```
fsm my_thing(x ∈ Int, y ∈ Int)     -- x, y are the interface (the axes)
    b ∈ Int = 5                     -- b is internal (a witness, projected out)
    y = b * x
```

## Why the interface is the right default

The key realization is an alignment between three distinctions that turn out to be
the *same* distinction:

| language | semantics | projection |
|---|---|---|
| **interface** var | observable (part of the relation's meaning) | dropping it **lies** |
| **internal** var | existential witness (eliminable) | dropping it is **honest** |

- An internal variable is existentially quantified — a witness to satisfiability,
  not part of the model's observable meaning. The model's semantics *is* the
  relation over the interface with internals projected out. So projecting an
  internal away isn't lossy — **it is the semantics**, and it's exactly what the
  functionizer does anyway.
- Projection only **lies** (the §3.3 caveat — joint structure collapses, apparent
  crossings that aren't real) when you drop a variable that is genuinely
  *observable*. The interface/internal split tells you precisely which projection
  is honest, and shrinks the real "which axes" choice to *among the interface
  variables only*.

And interface-only is usually **sufficient**, for three reasons:

1. **Internals aren't behavior.** They are common-subexpression-elimination for
   readability; the model means the same with them inlined. Nothing to "see."
2. **Feasibility / UNSAT is already visible at the interface.** The interface
   relation is a *set* — the feasible assignments, the holes where it goes UNSAT.
   That ("it's impossible *here* — why?") is the most useful internal insight, and
   it's an observable feature of the interface portrait, no box-opening needed.
3. **When the interface is too high-D, the answer isn't "look at internals"** — it's
   to define a better observable (a witness; below).

Internals-as-axes remains valid as a **white-box debug mode**, but it is secondary,
not the default.

## Witness / indicator variables

When two raw interface dimensions aren't a legible coordinate system, the right
move is not arbitrary projection — it's to **introduce a meaningful derived
coordinate and promote it to the interface**. The mechanism already exists: *put it
on the first line.*

```
fsm pendulum(theta ∈ Int, omega ∈ Int, energy ∈ Int)
    energy = (omega*omega)/2 - cos_approx(theta)    -- a witness coordinate
```

Now `energy` is observable and an available axis — even though it is "just" a
function of the others. The pendulum's energy is the archetype: a witness whose
**level sets *are* the orbits**, so a single energy axis makes the whole 2-D
portrait legible. Same family: a Lyapunov / ranking function (an axis the flow
descends → stability / progress), or a discrete **mode / phase indicator** enum
that collapses a messy numeric corner into "which regime."

Lineage: these are **ghost / specification variables** (Dafny/JML model fields) —
added purely to observe and specify, never part of the implementation — and
**order parameters** from dynamical systems. The mechanism is identical to an
internal variable (a named derived expression); the only difference is **intent**:
an internal is meant to be eliminated and hidden, a witness is meant to be kept and
watched. The interface line encodes that intent — on the line = observable, off it
= eliminable.

(We may later want distinct *syntax* for indicator vars, or a separate role, so
other models can pick them up. Not needed yet — promoting to the interface is
enough, and a useful indicator is usually worth being interface anyway.)

## The feedback loop (the point)

An **illegible portrait is a signal to introduce an indicator variable** — and
doing so improves *both* the picture *and* the source (now there's a named
`energy` / `phase` / `progress` a human reading the model benefits from too). The
visualization *drives* the language. This is the design doc's "the IDE is part of
the language," made concrete.

## The problem this does NOT solve: high interface dimensionality

Choosing the interface shrinks the candidate axes (drops internals) but does **not**
solve the core difficulty: **most fsm models carry many interface variables** (the
whole state *is* the interface). The "which 2, and what about the rest" question
remains. The honest options, by renderer family:

- **Whole-state-vector renderers** (`state_graph`, `morse_graph`,
  `reachability_tree`, `transition_matrix`) treat each full assignment as one
  atomic node — *no axis choice*, handles any dimensionality, but blows up
  combinatorially and the nodes are opaque composites.
- **All-variables renderers** (`time_series`, `timing_diagram`, `parallel_coords`,
  `scatter_matrix`) give every variable its own track/axis — *no projection*,
  scales to many vars (parallel coords / scatter matrix are *for* high-D).
- **2-axis projection renderers** (`phase_portrait`, `basin_map`, `orbit_scatter`,
  …) are the only ones that must *choose*, and today they choose by a crude
  heuristic. This is where the real work is, and the answers are:
  1. an explicit **axis spec** (which interface vars on the axes), defaulting to a
     heuristic;
  2. **faceting** — small multiples, one 2-axis panel per value of a discrete mode
     variable;
  3. **slicing** — fix the other interface vars to chosen values (a section);
  4. **quotient** — group by a predicate and draw the abstract graph (the design
     doc's quotient portrait);
  5. **witnesses** — the model supplies a good low-D coordinate (above).

## Implementation status

- `evident export` marks each carried state leaf `role: interface | internal`
  (interface = prefix is a first-line param). *(Today all our sample fsms' carried
  state is interface; the split matters for models with carried internal/witness
  state and for the debug mode.)*
- `viz/evident_viz.py`: `m.state_vars` = interface (the default axis set);
  `m.carried` = all carried (drives the transition queries); `m.internal_vars` =
  the rest. Renderers see the interface by default with no per-renderer change.
- **Open:** the 2-axis projection renderers still pick axes heuristically; an
  explicit axis-spec + faceting/slicing is the next increment. A white-box
  internals/debug mode is a later, opt-in addition.
