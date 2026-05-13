# Evident — Project Invariants

## What This Is

Evident is a constraint programming language where programs are collections of
constraints over sets, and a Z3 SMT solver finds satisfying assignments.  The
central abstraction is `schema`: a named set defined by membership conditions.
Querying a schema asks whether a satisfying assignment exists.

## Run `./test.sh` before declaring work done

There is one test command: **`./test.sh` from the repo root**.

It builds the runtime in release mode, runs `cargo test --release` (Rust
units + integration tests + the demo driver that runs every
`examples/test_*.ev` end-to-end), then runs `pytest tests/conformance/`
(black-box CLI conformance). All phases must pass; the script exits non-zero
if any phase fails.

The full run is **~10 seconds** when the binary is already built.

When to run it:
  * After any change that touches `runtime/`, `stdlib/`, `examples/`,
    or `tests/`.
  * Before the end of a multi-step task — even if you ran a subset of
    tests during the work, run the full thing once at the end.
  * If `./test.sh` fails, fix the failures before declaring done. Don't
    add `xfail` markers as a TODO; either fix the code or, if it
    surfaces a runtime gap, file an entry in
    `examples/COUNTEREXAMPLES.md` and delete the test.

Iteration-only flags:
  * `./test.sh --rust-only` — skip conformance phase.
  * `./test.sh --conformance` — skip the cargo build + cargo test
    phases (useful when iterating on Python conformance tests).
  * `./test.sh --examples` — phases 1–3 PLUS run every demo in
    `examples/` end-to-end via the binary, capturing
    screenshots for visual demos.
  * `./test.sh --examples-only` — just the examples runner;
    assumes the binary is already built.

The default — no flags — is what you should run before claiming work
is done.

### Visual verification of `--examples`

When `--examples` runs, it iterates every `examples/test_*.ev`:
- Non-visual demos run with a timeout, asserting clean exit.
- Visual demos (anything importing `packages/sdl/`) get spawned,
  given ~2s to draw, screenshotted to `/tmp/evident-screenshots/`,
  then killed.

The exit-code check covers correctness for non-visual demos but
**says nothing about whether visual demos render correctly** —
they could exit cleanly while showing a black window. The
agentic loop closes that gap:

  1. Run `./test.sh --examples`.
  2. List `/tmp/evident-screenshots/` to see which demos captured.
  3. For each PNG, use the Read tool (it accepts image paths and
     renders them inline). Visually verify the screenshot matches
     what the demo's docstring claims it should show — red window
     for `test_16_sdl_red`, RGB triangle for `test_17_sdl_triangle`,
     etc.
  4. If a demo renders something different from its docstring,
     either fix the demo, fix the runtime, or document the gap in
     `examples/COUNTEREXAMPLES.md`.

This is the only way visual regressions get caught — an agent
running `./test.sh --examples` and Reading the PNGs is functionally
the visual-test harness. We don't have a pixel-diff CI yet.

## Where to read first

Before writing code in this repo, check whether one of these guides covers
your task:

| If you're … | Read |
|---|---|
| Writing a new program (any program) | [`examples/`](examples/) — copy the closest existing demo's shape |
| Looking for the punch list of known runtime gaps | [`examples/COUNTEREXAMPLES.md`](examples/COUNTEREXAMPLES.md) |
| Writing or debugging a program that uses `evident effect-run` | [`docs/guide/effect-state-machines.md`](docs/guide/effect-state-machines.md) |
| Writing or extending an FFI wrapper library (`packages/sdl/`, `packages/gl/`, `stdlib/shell.ev`, …) | [`docs/guide/ffi-bindings.md`](docs/guide/ffi-bindings.md) |
| Understanding what an Evident model IS (the unifying framing) | [`docs/design/schema-interface.md`](docs/design/schema-interface.md) |
| Writing a multi-FSM program (cookbook) | [`docs/guide/multi-fsm-programs.md`](docs/guide/multi-fsm-programs.md) |
| Designing/extending the multi-FSM runtime, halt semantics, or scheduler | [`docs/design/multi-fsm.md`](docs/design/multi-fsm.md) + [`docs/design/fsm-subscriptions.md`](docs/design/fsm-subscriptions.md) |
| Trying to understand the architectural goals (~11K Rust target, FFI-first) | [`docs/design/minimal-runtime.md`](docs/design/minimal-runtime.md) |
| Designing the FFI primitive itself or extending it | [`docs/design/ffi-design.md`](docs/design/ffi-design.md) |
| Planning what to add to FFI / OS coverage (reads, writes, alloc, callbacks, posix) | [`docs/design/ffi-os-evolution.md`](docs/design/ffi-os-evolution.md) |
| Looking for plan files for the larger refactor | [`docs/plans/README.md`](docs/plans/README.md) |

The two `docs/guide/*` docs were written specifically to spare future-you
the painful debug sessions that produced them. If you're about to write a
state machine or an FFI binding, **read those first**.

## Conventions for `examples/` (this repo's test/example set)

These rules govern files we write into `examples/`. They
are NOT a property of the Evident language — a downstream user
writing their own Evident program is not bound by them. Inside
this repo, `examples/` is our canonical test set: every
file there doubles as a worked example AND an integration
test, so we hold them to a strict shape.

### 1. Demo files are integration tests

Each file in `examples/` is named `test_NN_<name>.ev`
and contains both:

  * The multi-FSM program (one or more `claim`s with state
    pair + `last_results ∈ ResultList` + `effects ∈ EffectList`).
    Single-FSM demos are written as multi-FSM programs with
    one FSM — the multi-FSM scheduler is the only execution
    path.
  * Inline `claim sat_*` / `claim unsat_*` static tests that
    pin state/inputs and assert on the FSM's response.

Two test runners cover both halves:

  * `evident test examples/` — discovers `test_*.ev`
    files, runs every `sat_*` / `unsat_*` claim.
  * `cargo test --test demos` (in `runtime/`) — runs
    each demo end-to-end via the binary, asserts on exit
    code and stdout substring. The `EXPECTATIONS` table in
    `runtime/tests/demos.rs` is the contract.

When adding a demo: drop the file in `examples/`, add a
row to `EXPECTATIONS`. Both runners stay green.

(The multi-FSM scheduler skips claims named `sat_*` / `unsat_*`
from auto-instantiation runtime-wide — they have the FSM
shape because they pin `state` / `effects` to assert
properties, but they should never run as FSMs. This applies
everywhere, not just to demo files.)

### 2. Demo files MUST NOT contain raw FFI calls

In any `examples/*.ev` file (and any other example we author),
`LibCall` / `FFICall` / `FFIOpen` / `FFILookup` are forbidden. Demos reach C code by either:

  * Calling **named claims** from `stdlib/` that wrap the FFI
    behind a typed interface. Example: `sdl_pump_events(out)` —
    not `out = LibCall("/opt/homebrew/lib/libSDL2.dylib", …)`.
  * Declaring **FTI typed resources** as parameters or body
    items (`win ∈ SDL_Window (title ↦ "X", …)`) and letting
    the runtime's bridge install own the C-side lifecycle.

If a demo needs a C function that no stdlib helper covers:
**add the helper to stdlib** (`stdlib/<library>/...ev`) first,
then call it from the demo. A demo file containing
`LibCall(...)` or a hardcoded library path like
`"/opt/homebrew/lib/libSDL2.dylib"` is a code-review blocker —
move it to stdlib.

The COUNTEREXAMPLES file lists what the runtime can't yet do
(e.g. SDL+GL render-via-dispatch). Don't work around those by
reaching into raw FFI from a demo; either fix the runtime, add
a stdlib wrapper, or document the limit.

(Outside `examples/` — your own application code, ad-hoc
exploration, etc. — these rules don't apply. They're a quality
bar for the canonical test set.)

## Language Definitions

The Rust runtime under `runtime/` is the only implementation. The
language is defined by the lexer + parser + AST + translator that
ship with it.

| Thing | Where defined |
|---|---|
| Lexer (Unicode operators → tokens) | `runtime/src/lexer.rs` |
| Parser (recursive-descent) | `runtime/src/parser.rs` |
| AST node types | `runtime/src/ast.rs` |
| AST → Z3 translator | `runtime/src/translate/` |
| Effect dispatch | `runtime/src/effect_dispatch.rs` |
| Multi-FSM scheduler | `runtime/src/effect_loop.rs` |
| FTI bridges | `runtime/src/event_sources.rs`, `runtime/src/fti.rs` |
| Stdlib (Evident) | `stdlib/` |
| Design docs | `docs/design/` |
| Worked examples + integration tests | `examples/` |

## Runtime Architecture

The runtime is a pipeline. Each stage is a separate module under
`runtime/src/`:

```
source text
  → lexer.rs              Unicode operators + word-keywords → tokens
  → parser.rs             Recursive-descent parser → AST (ast.rs)
  → translate/            AST → Z3 sorts + constraints; per-claim inline
  → runtime.rs            EvidentRuntime: top-level API (load_file, query)
  → effect_loop.rs        Multi-FSM scheduler (the executor)
  → effect_dispatch.rs    Effect → IO (Println, LibCall, ParseInt, …)
  → event_sources.rs      FTI bridge implementations (one struct per
                          typed C resource)
  → fti.rs                FTI registry: type-name → install fn
```

Supporting modules:
- `subscriptions.rs` — static read/write-set inference per claim
- `ffi.rs` — libffi marshaling, handle registry
- `pretty.rs` — AST printer for diagnostics
- `commands/` — per-CLI-subcommand entry points

## Multi-FSM Runtime

For programs run via `evident effect-run`, the multi-FSM scheduler
in `runtime/src/effect_loop.rs` runs each top-level claim matching
the FSM shape (state pair + EffectList + ResultList) as an
independent FSM.

**Scheduler: subscription-driven (default).** An FSM ticks only when one of
its inputs changes:
  * **World read-set** — fields it references via `world.X` (auto-inferred
    by `subscriptions::world_access_sets`). Wakes when another FSM writes.
  * **Effect self-feedback** — emitted ≥1 effect last tick.
  * **State self-feedback** — transitioned to a new state value last tick.
  * **Bootstrap** — every FSM ticks once on tick 0.

**Halt is implicit.** No `Done`/`Halt` name convention, no fixpoint
heuristic. The program halts when no FSM was scheduled in a tick (nothing
more can happen) or when any FSM emits `Effect::Exit(code)`.

**`Effect::Exit(code)` is graceful** — it sets `exit_requested` on the
dispatch context. The runtime dispatches all of the current tick's
effects first (so co-scheduled FSMs' cleanup writes / final logs run),
then halts at end-of-tick with the requested code. `LoopResult::exit_code`
propagates to the CLI as the process exit code.

**Opt out** with `EVIDENT_SCHEDULER=legacy` for the older "tick every FSM
every iteration" behavior with name/fixpoint halt.

**Async event sources.** When no FSM is ready to tick (all subscriptions
silent), the scheduler blocks on a channel of `SchedulerEvent`s instead
of immediately halting.

There are TWO ways an FSM subscribes to async events:

  1. **Plugin-as-writer (preferred, unified model)** — the user's
     `World` type declares a reserved field; the runtime auto-installs
     a plugin to write that field. User FSMs subscribe via existing
     world read-set inference. No marker types, no event-channel API.
       * `tick_count: Int`     → FrameTimer (set rate via `EVIDENT_TICK_MS=<u64>`)
       * `signal_received: Int` → SigintSource (auto-installed)
       * `stdin_line: String`  → StdinSource (auto-installed)
       * `stdin_seq: Int`      → StdinSource also writes seq counter
     Plugin writes participate in the multi-writer disjoint check
     — a user FSM trying to write a plugin-owned field is rejected
     at load.

  2. **Marker-type subscription (legacy v3 path)** — an FSM has a
     parameter of type `FrameTimer` / `Signal` declared in
     `stdlib/runtime.ev`. The plugin pushes wake-only events;
     the FSM body has no payload to read. Useful when the user
     wants to be woken without making the source's value part of
     world.

If NO FSM declares any subscription, falls back to coarse wake for
back-compat. When all sources go dead (channel returns Err), the
scheduler halts cleanly. See `runtime/src/event_sources.rs` for
the `EventSource` trait — adding a new source is implementing the
trait + wiring it into `run_with_ctx`.

**Sources are FSMs too.** Each event source is a stateful state
machine implemented in Rust — same coordination model as user FSMs,
different language. User FSMs talk to source FSMs via effect emission
(commands) and `last_results` / wake events (responses). v1 sources
are push-only (events flow source → owner; no commands). v2 will add
bidirectional command channels (mode switching, explicit reads,
seeks, close).

**Single-owner per fd-style resource.** Stdin, sockets, files, child
processes — every fd-shaped resource has exactly one owner FSM
(declared via marker type), enforced at load time. The owner reads,
parses, publishes to world. Downstream FSMs read world; they never
touch the resource directly. Sharing an fd across FSMs without
coordination is the same race-on-read footgun that bites C programs;
the runtime refuses to allow it. See `docs/design/fsm-subscriptions.md`
"The runtime is an FSM too" for the full framing.

**Design**: [`docs/design/multi-fsm.md`](docs/design/multi-fsm.md) covers
the writer/reader pattern + worked examples; [`docs/design/fsm-subscriptions.md`](docs/design/fsm-subscriptions.md)
covers the scheduler model and 5-phase implementation status.

## Keyword Conventions

All three keywords — `type`, `claim`, and `schema` — produce identical AST nodes
(`SchemaDecl`) and are interchangeable at the runtime level.  The distinction is
a reading contract described in `docs/design/what-we-learned.md`:

**`type`** — Use for things that define the structure of a single record value.
A type is a noun: something you instantiate and hold.  The constraints inside it
are simple local invariants on its own fields — always true for any valid instance,
no external dependencies.

```evident
type GameState
    location  ∈ String
    inventory ∈ Seq(Item)
    turn      ∈ Nat

type DateRange
    start ∈ Date
    end   ∈ Date
    start ≤ end        -- local invariant on DateRange's own fields
```

**`claim`** — Use for relations across multiple values, traits, properties, and
constraint modules.  A claim is a predicate: something that holds or doesn't hold
for a given set of values.  Claims are used both in test files (as assertions to
verify) and as constraint modules that can be mixed into other claims or types.
The test-file convention `sat_*` / `unsat_*` is just one application.

```evident
-- Trait / constraint module: a reusable property
claim assignment_fits_schedule
    a        ∈ Assignment
    schedule ∈ Set Assignment
    ∀ b ∈ schedule : a.room = b.room ⇒ a.slot ≠ b.slot

-- Test assertion
claim sat_north_exit_exists
    ("entrance", "north", "forest") ∈ exits_map
```

The practical line: if the constraints are purely local to the type's own fields
→ `type`.  If they involve external data, multiple objects, or complex logic that
varies by context → `claim`.

**`schema`** — Avoid.  It is a synonym for `type` with no additional meaning.
Prefer `type` when the thing is a noun (has a shape); prefer `claim` when it is a
predicate (defines a relation or property).  The word `schema` does not appear in
human-written Evident source files.

**`..TypeName` (passthrough / trait composition)** — Brings another type's or
claim's fields and constraints directly into the current scope without a dotted
prefix.  Think of it as trait composition.  The included declaration is still a
`type` or `claim`; `..` is the composition mechanism.

## Composing Types and Claims

### Using a type inside a claim: `variable ∈ TypeName`

Declares a variable of that type.  All of the type's fields become accessible
as `variable.field`, and the type's invariants are automatically enforced.
Use this when a claim needs to reason about a structured object.

```evident
claim assignment_fits_schedule
    a        ∈ Assignment      -- a is an Assignment; a.room, a.slot available
    schedule ∈ Set Assignment
    ∀ b ∈ schedule : a.room = b.room ⇒ a.slot ≠ b.slot
```

### Using a claim inside a type: baking a property in

When every instance of a type should satisfy a property, put the claim's
name directly in the type body.  The names-match rule identifies variables
automatically.

```evident
type ValidSchedule
    slots   ∈ Seq(TimeSlot)
    budget  ∈ Nat
    no_conflicts     -- claim; 'slots' matches by name
    within_budget    -- claim; 'budget' matches by name
```

This creates a **refined type** — a subset of all schedules that satisfy
those additional properties.  Use it when the constraint should always hold
for any valid instance, with no external data needed.

### Passthrough `..`: flat mixin, no prefix

`..SomeType` or `..SomeClaim` brings all fields into the current scope
without a dotted prefix.  The included constraints also apply.

```evident
type main
    ..LineReader    -- adds line, line_ready, src.* directly into scope
    ..LineWriter    -- adds line_out, dst.* directly into scope
    state ∈ GameState
```

Use passthrough when the fields of the included type/claim ARE fields of
the current type — not a sub-object you reference by name.  `..LineReader`
makes `line` available directly; `reader ∈ LineReader` would make it
`reader.line`.

### Names-match composition: zero-argument claims

When variable names in scope match a claim's variable names, just name the
claim — no explicit argument passing needed.  The solver identifies them.

```evident
claim valid_conference
    schedule     ∈ Set Assignment
    rooms        ∈ Set Room
    max_parallel ∈ Nat

    rooms_conflict_free    -- 'schedule' flows automatically by name
    parallel_load_within   -- 'schedule', 'max_parallel' flow by name
```

### Interface vars on the claim line + positional invocation

When a claim takes parameters, put them on the claim line (first-line
params) so callers can use **positional invocation** without `mapsto`:

```evident
claim Distinct(s ∈ Seq, n ∈ Nat)
    ∀ i ∈ {0..n-1} : ∀ j ∈ {0..n-1} : i < j ⇒ s[i] ≠ s[j]

claim my_problem
    items ∈ Seq(Int)
    #items = 8
    Distinct(items, 8)             -- positional, no `mapsto` needed
```

The claim's first-line params define its **interface** — the variables
the caller must supply. Other vars declared in the body are
internal. This shape:

  - Reads like a function signature.
  - Saves verbosity at every call site (no `slot ↦ value` ceremony).
  - Lets the same claim be invoked with different caller-side names
    (no need for the caller's vars to match the claim's slot names).

Compare:

```evident
-- Verbose: claim has body-level decls, caller uses mapsto OR
-- must match names exactly:
claim Distinct
    s ∈ Seq
    n ∈ Nat
    …
Distinct (s ↦ items, n ↦ 8)        -- mapsto every call
-- or
items_renamed_to_s ∈ Seq(Int)       -- contort the names
Distinct                             -- bare names-match

-- Compact: interface on the claim line, positional at call site:
claim Distinct(s ∈ Seq, n ∈ Nat)
    …
Distinct(items, 8)                   -- one short call
```

**Rule of thumb**: any var the caller needs to supply belongs on the
claim line. Internal helpers (intermediate Reals/Bools, named
sub-results) stay in the body.

### Generic Seq parameters: `s ∈ Seq` (no element type)

A claim parameter declared as `s ∈ Seq` (bare, no element type) takes
its element type from the caller's binding via names-match. The same
claim then works for any orderable / equality-comparable element type:

```evident
claim Distinct
    s ∈ Seq                  -- generic; element type comes from caller
    n ∈ Nat
    ∀ i ∈ {0..n-1} : ∀ j ∈ {0..n-1} : i < j ⇒ s[i] ≠ s[j]

claim use_int
    s ∈ Seq(Int)
    n ∈ Nat
    n = 4 ; #s = n
    s[0] = 7 ∧ s[1] = 2 ∧ s[2] = 9 ∧ s[3] = 4
    Distinct                 -- works on Int

claim use_string
    s ∈ Seq(String)
    n ∈ Nat
    n = 3 ; #s = n
    s[0] = "a" ∧ s[1] = "b" ∧ s[2] = "c"
    Distinct                 -- same claim, works on String
```

The runtime infers the element type at inline time from whatever the
caller declared. Body operations (`s[i] ≠ s[j]`, `a ≤ b`, etc.) get
translated against the caller's type. `stdlib/distinct.ev` and
`stdlib/sorted.ev` use this pattern — single generic claim, not
per-type variants.

Use this whenever a claim's logic doesn't depend on the specific
element type — distinctness, sortedness, bijection between two seqs,
sum-of-elements, etc. Don't use it when the body's translation
depends on the type (e.g., a claim that only makes sense for Bool
sequences) — give it a concrete `Seq(Bool)` so the type-check fires
at the call site.

### Chained-membership: declare and constrain on one line

`∈` can sit inside a chained-comparison expression at the body-item
level. The variable to its left gets declared with the type to its
right, and every comparison pair in the chain becomes its own
constraint. Three common shapes:

```evident
pos_x ∈ Int = 5            -- declare + pin
pos_x ∈ Int < 5            -- declare + upper bound
0 < pos_x ∈ Int            -- declare + lower bound
0 < pos_x ∈ Int < 5        -- declare + range  (replaces 3 lines)
0 ≤ score ∈ Nat ≤ 100      -- any comparison ops work
val ∈ Int ≠ 0              -- inequality after declaration
```

Each desugars to a `Membership` plus one `Constraint` per comparison
pair. `0 < pos_x ∈ Int < 5` becomes:

```evident
pos_x ∈ Int
0 < pos_x
pos_x < 5
```

Multi-name shorthand works in chains too — every comparison pair
gets a per-name copy:

```evident
x, y, z ∈ Int < 5          -- 3 Memberships + 3 Constraints (each < 5)
0 < x, y, z ∈ Int < 5      -- 3 Memberships + 6 Constraints (lower + upper per name)
```

The variable being declared must be a bare identifier (no field
access — `state.x ∈ Int < 5` is rejected). Compound types work
without comparisons (`s ∈ Seq(Int)` parses normally) but the
chained form expects a plain type name on the right of `∈`.

The chain detector requires the next token after the chain to be a
line-end. Constraints joined with `∧`/`∨` like `x ∈ pts ∧ x > 0`
still parse as expressions (set-membership inside a Bool), not as
chained-membership.

### Renaming with `↦`: when names differ

```evident
claim manage_event
    assignments ∈ Set Assignment
    Conference.valid (schedule ↦ assignments)  -- rename to match
```

### `subclaim`: nested claim scoped to a parent

A `subclaim` is a claim definition nested inside another claim's body.  It
has access to all of the parent claim's variables by name, but its
own internal variables are fresh and not visible to the parent.

```evident
claim GameTransition
    state      ∈ GameState
    state_next ∈ GameState
    response   ∈ String
    verb       ∈ Verb

    subclaim LookAction
        -- state, state_next, response, verb are inherited from parent
        state_next.location = state.location
        (state.location, room_desc) ∈ room_descriptions
        response = room_desc

    subclaim GoAction
        -- direction, dest are internal to GoAction — not in parent scope
        direction ∈ String
        dest      ∈ String
        (state.location, direction, dest) ∈ direction_exits
        ...
```

Use subclaims when a claim's dispatch logic is complex enough to name,
but the branches are implementation details not independently composable.

### `⟸` (reverse implication): dispatch tables

`A ⟸ B` means `B ⇒ A` (A applies when B).  It's syntactic sugar that
makes verb-dispatch tables read naturally:

```evident
-- "GoAction applies when verb = Go"
GoAction ⟸ verb = Go

-- Equivalent (but reads backward):
verb = Go ⇒ GoAction
```

Use `⟸` in dispatch tables where the consequent is named and the
condition is the selector.

### Decision guide

| Situation | Pattern |
|---|---|
| A claim needs one structured object | `variable ∈ TypeName` in the claim |
| A type should always satisfy a property | name the claim in the type body |
| Fields should live flat in scope (no prefix) | `..TypeName` or `..ClaimName` |
| Reusing a claim whose variable names match | name it directly (names-match) |
| Reusing a claim with different variable names | name it with `(x ↦ y)` |
| A subset of a type with extra invariants | define a new `type` that names the original type and adds constraints |
| Named dispatch branches inside a parent claim | `subclaim` + `⟸` |
| Multiple variables sharing a type | `x, y, z ∈ Int` (multi-name shorthand) |
| Declare and constrain in one line | `pos_x ∈ Int = 5`, `pos_x ∈ Int < 5`, or `0 < pos_x ∈ Int < 5` (chained-membership) |
| Compact short-record type definition | `type IVec2(x, y ∈ Int)` (first-line param list) |
| Construct a record value inline | `IVec2(380, 280)` positional, or `IVec2(x ↦ 1, y ↦ 2)` named |
| Componentwise comparison/equality of records | `a ≤ b`, `a = b`, `a ≠ b` lift automatically |
| Record-valued arithmetic equation | `c = a - b` lifts componentwise |
| Bounding-box / chained range on a record | `lo ≤ vec ≤ hi` (vector chained comparison) |
| Iterate parallel sequences | `∀ (a, b) ∈ coindexed(seqA, seqB) : …` |
| Iterate consecutive pairs of one sequence | `∀ (a, b) ∈ edges(seq) : …` |
| Inline a claim only when a condition holds | `cond ⇒ ClaimName` (guarded invocation) |
| Pin some fields of a record at declaration | `name ∈ Type (slot ↦ v)` or `name ∈ Type(v1, v2)` |
| Choose between two values based on a condition | `(cond ? a : b)` — ternary; both branches same sort, lowers to Z3 `ite` |
| Pattern-match an enum-typed scrutinee | `match e \n   Ctor(b) ⇒ body \n   _ ⇒ fallback` — indented arms, lowers to nested ITE |
| Test whether an enum value's variant is X (Bool result) | `e matches Ctor(_, _)` — recognizer; payload binds ignored. Use `match` to extract values, `e = Ctor(7)` for literal-payload comparison |
| Build a `Cons/Nil`-shaped enum value (EffectList, ResultList, ArgList, user LinkedList) | `var = ⟨a, b, c⟩` — lowers to `Cons(a, Cons(b, Cons(c, Nil)))`. Empty `⟨⟩` = `Nil`. Works inline in `match` arms when the LHS hints the enum type |

## Records as vectors

A short record type used as a value carrier (positions, colors, sizes,
velocities) gets first-class support throughout the runtime. Define
the type once with the multi-name shorthand:

```evident
type IVec2(x, y ∈ Int)
type Color(r, g, b ∈ Nat)
```

Once defined, four lifting forms work automatically:

**1. Componentwise comparison and equality**
```evident
pos_lo ≤ dot.pos ≤ pos_hi    -- pos_lo.x ≤ pos.x ≤ pos_hi.x ∧ same for y
a < b                         -- componentwise (every axis strict)
a = b                         -- componentwise
a ≠ b                         -- some-field-differs (disjunctive)
```

**2. Arithmetic broadcast in equation context**
```evident
c = a - b                     -- c.x = a.x - b.x ∧ c.y = a.y - b.y
nxt.pos = cur.pos + cur.vel * input.dt / 1000
state_next.dots[i] = src       -- whole-element record assignment via Index LHS
```

The lift sees `Identifier`, `Field-of-Index`, and `Index` records
(e.g. `dots[i]`), composes through `Binary` arithmetic, and
substitutes per-leaf. Shape mismatches (Vec2 = Vec3, etc.) are fatal
via the dropped-constraint policy — no silent partial-overlap.

**3. Type-use pins at declaration sites**
```evident
vel_lo ∈ IVec2 (x ↦ -800, y ↦ -800)   -- named, order-independent, partial allowed
pos_hi ∈ IVec2(740, 540)               -- positional, declaration order
sky    ∈ Color(30, 80, 120)
```

Both desugar to declaration + per-field equality. Named is partial
(omit fields to leave them free); positional requires args ≤ field
count and pins the leading fields.

**4. Record literals in expression position**
```evident
state.player.pos = IVec2(380, 280)
rect.pos   = dot.pos - IVec2(12, 12)
rect.color = Color(80, 200, 180)
```

Same `Type(args)` syntax as positional pins, used as a value-producing
expression. Lifts through equality and arithmetic identically to the
declaration form. Also valid as an inline argument to a claim call —
positional or `mapsto`:

```evident
set_draw_color(ren, Color(220, 40, 60, 255), out)   -- positional
use_color (c ↦ Color(7, 8, 9), sum ↦ s)             -- mapsto
```

The runtime expands the literal per-field and binds `slot.field` to
each arg before inlining the claim's body.

## N-arity sequence iteration

`coindexed(seqA, seqB, …)` zips parallel sequences by index;
`edges(seq)` iterates adjacent `(seq[i], seq[i+1])` pairs. Both use
tuple binding and require pinned lengths.

```evident
∀ (cur, nxt) ∈ coindexed(state.dots, state_next.dots) :
    nxt.pos = cur.pos + cur.vel * input.dt / 1000

∀ (cur, nxt, eff) ∈ coindexed(state.dots, state_next.dots, effective_vy) :
    -- per-dot physics referencing both snapshots and a parallel intermediate

∀ (a, b) ∈ edges(items) : a ≤ b   -- monotonicity
```

**Always prefer these over `∀ i ∈ {0..#seq - 1}` indexed loops.** The
tuple binding makes "what's being paired" visible at the call site;
the integer index never appears in the body.

**Caveat: parallel-Seq lengths must be pinned in `type main`'s body.**
The seq-length pinning preprocessor (`collect_seq_lengths`) only scans
the entry schema's body items. Seqs declared inside subclaims or
referenced through claim parameters won't have their `coindexed`
length pinning visible. Declare the Seqs in main, even if only an
inner subclaim uses them.

## Guarded claim invocation

`condition ⇒ ClaimName` inlines the claim's body but wraps each
constraint in `condition ⇒ …`. Declarations from the claim fire
unconditionally; only constraints get guarded. Composes with
names-match — the claim's parameters resolve to outer-scope variables
of the same name without explicit `mapsto`.

```evident
claim InitGameState
    state ∈ GameState
    input ∈ SDLInput
    init_seeds ∈ Seq(Int)
    -- … initialization constraints …

type main(state, state_next ∈ GameState)
    input ∈ SDLInput
    init_seeds ∈ Seq(Int)
    -- … other setup …
    state.step = 0 ⇒ InitGameState   -- runs Init's constraints only on frame 0
```

Useful for one-shot setup ("first frame"), conditional behavioral
modes, or anywhere you'd otherwise inline a guard onto every
constraint of a named concern.

## Style: keep main compact

`type main` should read as **setup + configuration + claim wiring**,
not as a place where logic lives. Aim for ~80–100 lines for a
non-trivial game/simulation. Five tools that compound:

1. **Multi-name + first-line params for short types** —
   `type IVec2(x, y ∈ Int)` over four lines.
2. **Positional pins for short type instantiation** —
   `pos_lo ∈ IVec2(20, 20)` over two field equalities.
3. **`coindexed(...)` / `edges(...)` over indexed loops** — drop
   `∀ i ∈ {0..#seq - 1}` whenever the body operates on parallel-seq
   elements at the same index, or on adjacent pairs.
4. **Extract per-frame concerns into claims** — bounds, physics,
   render, collection, win, audio each become a one-line invocation
   from main; the claim body owns the `∀` and the per-element logic.
5. **Guarded claim invocation for one-shot logic** — `state.step = 0
   ⇒ InitGameState` reads as "run Init when initializing".

(Earlier `sdl_demo/` engine + game pair is gone — the canonical
split is now embodied across `examples/test_NN_*.ev`. When we
build a richer game demo it should follow the same shape: an
engine claim file in `stdlib/` for reusable per-frame logic,
the game-specific types and aesthetic choices in the demo file.)

### Comments

Names carry the meaning. Section headers with one-line context are
fine; do not paragraph-explain every constraint. Counter-example to
avoid:

```evident
-- Update the dot's x position by adding velocity * dt to current.
nxt.pos.x = cur.pos.x + cur.vel.x * input.dt / 1000
```

The code already says this. Comment when the WHY isn't obvious — a
hidden invariant, a runtime caveat, an "I tried the obvious thing and
it broke" note. Otherwise let the names speak.

## Program Structure

Full guidance: `docs/design/program-structure.md`. Summary below.

### The layered stack

```
data layer     — enums and complete lookup tables (ground facts, no logic)
type layer     — pure structs and state snapshots (local invariants only)
trait layer    — small reusable behavioral claims
claim layer    — relations, dispatch, transition systems
entry point    — wiring only (passthroughs + variable declarations)
```

Each layer depends only on layers below it. The entry point (`type main`) should
contain no logic — only passthrough composition and variable declarations.

### Boolean literals are lowercase

`true` and `false` (lowercase). `True` and `False` (capitalized) parse as
unbound identifiers — the constraint is silently dropped and the variable
is left free. This produces no error, just wrong behavior.

```evident
state_next.done = true    -- correct
state_next.done = True    -- SILENT BUG: True is an unbound name, constraint dropped
```

### Precedence: `⇒` binds tighter than `∧`

**This is a footgun.** Evident's grammar has `⇒` at higher precedence than `∧` —
opposite of standard mathematical convention. So:

```evident
A ⇒ B ∧ C        -- parses as (A ⇒ B) ∧ C  ← wrong!
A ⇒ (B ∧ C)      -- correct: parentheses required for compound consequent
```

In a dispatch table, every consequent with multiple terms needs parens:
```evident
parsed.verb = Look ⇒ (StateTurn ∧ LookAction)   -- correct
parsed.verb = Look ⇒ StateTurn ∧ LookAction      -- WRONG: LookAction fires unconditionally
```

Alternatively, use an implies_block (indented body) to avoid the issue:
```evident
parsed.verb = Look ⇒
    StateTurn
    LookAction
```

### Precedence: `=` binds tighter than `∧` / `∨`

**Same family of footgun.** A boolean assignment that mixes `=` with logical
operators on the RHS needs outer parens or it splits into the wrong shape:

```evident
in_box = abs(x - cx) ≤ w ∧ abs(y - cy) ≤ h     -- WRONG
-- parses as ((in_box = abs(x-cx)) ≤ w) ∧ (abs(y-cy) ≤ h)
-- — a free-floating boolean expression, in_box is never assigned

in_box = ((abs(x - cx) ≤ w) ∧ (abs(y - cy) ≤ h))   -- correct
-- the outer parens scope `∧` inside the equation's RHS
```

Comparison operators (`<`, `>`, `≤`, `≥`) are also looser than `=`:

```evident
in_circle = length(p - c) < r       -- WRONG, parses as ((in_circle = length(...)) < r)
in_circle = (length(p - c) < r)     -- correct
```

Rule of thumb in shader bodies (or anywhere you assign a boolean result):
**always wrap the RHS in `( )` if it contains `<`, `>`, `≤`, `≥`, `∧`, `∨`, or
multiple `=` signs**. Costs nothing and the parser will tell you if you wrote it
wrong.

### The complete lookup pattern

Partial lookup tables cause Z3 non-determinism. If a table only has entries for
valid cases, Z3 can satisfy `(A, B, result) ∈ table ⇒ body` by choosing a
non-matching `(A, B)` to make the antecedent false.

Fix: make every table complete, using a sentinel (e.g. `""`) for "nothing":
```evident
assert direction_exits = {
    ("entrance", "north", "forest"),
    ("entrance", "south", ""),     -- blocked: sentinel, not absent
    ...
}
```
Then branch positively on the result: `dest ≠ "" ⇒ ...` / `dest = "" ⇒ ...`.

### Variable scope planning

Parent-level variables = the public interface (everything subclaims share).
Subclaim-internal variables = implementation details used by one branch only.

If a variable appears in only one subclaim, declare it inside that subclaim
(it becomes a fresh Z3 constant, not visible to the parent or other subclaims).

### Constraint scope rule

**Constraints referencing external data cannot live in a type body.**

When `item ∈ Item` is expanded, the sub-env contains only Item's own fields.
A constraint like `(kind, name) ∈ item_names` would be silently dropped because
`item_names` is not in that sub-env. Move it to the claim where the global fact
is in scope.

### Naming conventions

- **Enums**: `ItemKind`, `Verb` — name the set of identity values
- **Pure structs**: `Item`, `ParsedCommand` — noun, no external constraints
- **Traits**: `PreservesInventory`, `AdvancesTurn` — adjective/present-participle
- **Action subclaims**: `LookAction`, `GoAction` — noun phrase naming the branch
- **Dispatch**: `ActionName ⟸ condition` reads "ActionName applies when condition"

### Diagnostic questions

- Are all lookup tables complete? Any partial table risks Z3 non-determinism.
- Do any type bodies reference lookup tables? Move those constraints to the claim.
- Are there variables that always appear together? They may be a type.
- Are there repeated constraint patterns across branches? They may be a trait.
- Can you name each dispatch branch? If not, it may need further decomposition.
- Does the parent declare variables only one subclaim uses? Move them inside.

## I/O Plugins

The executor is one loop. Side-effectful I/O is handled by plugins, each
claiming one or more Evident type names. Plugins live in `runtime/src/plugins/`
and inherit from `runtime/src/plugin.py:Plugin`.

**Built-in plugins:**

| Plugin | Type names |
|---|---|
| `StdinPlugin`     | `Stdin`, `CharInput` — one char per step |
| `StdoutPlugin`    | `Stdout`, `Stderr`, `CharOutput` — write `var.out` per step |
| `BatchInputPlugin`  | `StdinLines`, `StdinAll`, `StdinChunks` — one-shot |
| `BatchOutputPlugin` | `StdoutLines`, `StdoutAll` — one-shot |
| `SDLPlugin`       | `SDLInput`, `SDLOutput` — graphical window |

**Auto-detection.** `executor.run()` calls `plugin.initialize(declared_vars)`
on every plugin in the default list; only those whose `handles_types`
matches at least one variable in `main` become active. Programs that
declare `∈ Stdin` get the StdinPlugin; programs that declare `∈ SDLOutput`
get the SDLPlugin; programs that declare both get both.

**Lifecycle.** `start()` once at the beginning, `before_step()` and
`after_step()` per step, `stop()` once at shutdown (in a `finally` block).
`before_step → None` and `after_step → False` both signal halt.

**Adding a plugin.** Subclass `Plugin`, set `handles_types = {...}`, override
the lifecycle methods you need, then add an instance to `default_plugins()`
in `runtime/src/plugins/__init__.py`. The executor handles the rest.

**Footgun: blocking I/O.** If a program declares both `∈ Stdin` and
`∈ SDLInput`, the StdinPlugin's `before_step` blocks waiting for a character,
which freezes the SDL window. Single-source-of-input is the supported case.
Future: a "non-blocking" plugin trait or `select()` on stdin when SDL is also
active.

## Key Invariants

**Parser**
- The grammar is the single source of truth for syntax.  The normalizer runs
  first and converts Unicode operators to `__TOKEN__` form before Lark sees the
  source, so the grammar only contains ASCII tokens for operators.
- `normalizer.py` maps both directions: Unicode symbols *and* word keywords
  (`in`, `not in`, `subset`, `superset`, `mapsto`) to the same `__TOKEN__`.
  Adding a new keyword requires updating the normalizer *and* the grammar.

**AST**
- Runtime files import AST types from `runtime/src/ast_types.py`, not directly
  from `parser/src/ast.py`.  `ast_types.py` re-exports via a proper package
  import so all code shares one class identity — two separate `importlib.util`
  loads produce different class objects and break `isinstance`.

**Sorts and enums**
- `SortRegistry` is the single owner of all Z3 sorts and enum constructors.
- Enum variant names are **global** and must be unique across all enum types.
  `declare_algebraic` raises `ValueError` on duplicate variant names.
- **Python**: `type Color = Red | Green | Blue` declares a named enum.
- **Python only**: `x ∈ Red | Green | Blue` (inline enum) auto-declares an
  anonymous enum named `_Enum_<sorted_variants>` and is equivalent to declaring
  the type separately.
- **Rust**: top-level `enum Color = Red | Green | Blue` with the dedicated
  `enum` keyword (not `type`). Payload variants, self-recursion, forward
  references, and **cross-enum mutual recursion** are all supported:
  `enum Result = Ok(Int) | Err(String)`,
  `enum LinkedList = Nil | Cons(Int, LinkedList)`, and
  `enum Expr = ENum(Int) | EBlock(Stmt) ; enum Stmt = SExpr(Expr) | SSeq(Stmt, Stmt)`
  all work. Multiple enum decls per file are batched and built together via
  Z3's `create_datatypes` so forward and mutual references resolve in one
  pass. Multi-line variant lists are supported (with or without leading `|`).
  Constructors apply with positional args: `r = Ok(5)`,
  `list = Cons(7, Cons(2, Nil))`. Variant names are globally unique across
  all enums; duplicates fail at load.

**Variable scoping**
- Variables declared inside a schema (`x ∈ Nat`) are local to that schema's
  query.  Independent queries do not share environments.
- Composed sub-schemas get a dotted prefix: `task ∈ Task` expands into
  `task.id`, `task.duration`, etc. in the parent environment.  The bare `task`
  variable is not created; only the leaf fields exist in Z3.
- Type names (e.g. `Color`) can be reused as variable names inside a schema
  without conflict — they occupy different namespaces.

**Subclaims**
- `subclaim Name ... ` inside a claim body defines a locally-scoped claim.
  It is registered into `self.schemas` by `runtime.py`'s `load_schema` at
  load time, so it is available for names-match composition even when the
  parent is used via passthrough (not directly evaluated).
- Subclaim-internal variables (declared inside the subclaim body but not in
  the parent scope) receive fresh Z3 constants via `z3.FreshConst` in
  `translate.py`'s claim composition code.  They are not visible to the parent.
- Adding a subclaim: define it in the parent body; it is automatically picked up.

**Z3 safety**
- Z3's C library is not safe for concurrent use from multiple threads.
- The IDE backend runs `/sample` and `/ranges` in isolated subprocesses via
  `ide/backend/z3_worker.py` to prevent server crashes.
- `/ranges` results are cached (LRU, 128 entries) keyed by request hash.
  `/sample` is intentionally **not** cached — results are random.
- Push/pop inside a single subprocess is safe.  Never use push/pop across
  request boundaries in the web server process.

**Sub-schema field access**
- `task.duration` in source is parsed as `BinaryExpr(×, Identifier('task'),
  FieldAccess('.', 'duration'))` by the grammar (juxt-dot ambiguity).
  `translate.py` intercepts this pattern and resolves it as a dotted env
  lookup before evaluating operands.

## IDE

```
ide/
  backend/
    main.py          FastAPI app; /parse, /evaluate, /ranges, /sample, /transfer
    z3_worker.py     Subprocess worker for Z3 isolation
    ranges.py        Binary-search minimum finder (no Z3 Optimize)
    sampler.py       blocking_clause_sample, random_seed_sample, grid_sample
  frontend/
    editor.js        Monaco setup + LaTeX-style keyword→symbol substitution
    evident-lang.js  Monaco Monarch tokenizer + dark theme
    schema-panel.js  Schema selector and variable binding inputs
    samples.js       Sample table; accumulates unique samples across runs
    ranges.js        Variable range bars
    scatter.js       2D plot: scatter (num×num), strip (enum×num), count bars (enum)
  tests/
    test_ide.py      Playwright end-to-end tests (server must be on :8765)
```

**Running the IDE**

```bash
uvicorn ide.backend.main:app --port 8765
# then open http://localhost:8765/app/
```

**Running tests**

```bash
pytest runtime/tests/ parser/tests/     # unit tests (fast, ~2s)
pytest ide/tests/test_ide.py            # Playwright e2e (requires server on :8765)
```
