# Evident — Project Invariants

These are the load-bearing rules for this project. They do not
change between sessions. Status snapshots (what works today, what's
in flight) belong in `docs/plans/`, not here.

> **🚀 New session?** After reading this file, read
> **`docs/plans/NEXT.md`** for the current handoff state +
> concrete next-step proposals + discovered language gaps with
> workarounds. Then `docs/plans/iter-3-status.md` for what each
> iteration demonstrated.

## The end-goal invariant

**The Rust runtime in `runtime/` is frozen in spirit. New work goes
into Evident, not into Rust.**

This is the project's north star. Today the Rust runtime is still
the compiler; eventually it gets bootstrapped out and deleted. Every
session should move toward that end. If you find yourself wanting to
modify `runtime/src/`, stop and ask whether the same effect can be
achieved in:

1. `stdlib/*.ev` — Evident library code
2. `kernel/src/` — the minimal native runtime (allowed but minimal)
3. A new pass written in Evident, run by the kernel

If none of those work, the missing capability is a *runtime-extension
proposal*, not a casual change. Document it in `docs/plans/` and get
the user's review before modifying `runtime/`.

This invariant gets stricter as iter 3+ progresses. By the end,
**zero changes to `runtime/`** is expected.

## What this project is

Evident is a constraint programming language where programs are
collections of constraints over named variables. A Z3 SMT solver
finds satisfying assignments. The central abstraction is `schema` (or
`type` / `claim`): a named set defined by membership conditions.

A program in Evident:
1. Defines schemas + their members.
2. The compiler translates to SMT-LIB.
3. The kernel runs the SMT-LIB: solves, dispatches effects, repeats.

## Project structure

```
runtime/     — Bootstrap Rust compiler (FROZEN — do not modify lightly)
kernel/      — Trampoline + libffi runtime (Rust, stays Rust)
stdlib/      — Evident library code (Effect enum, builders, …)
tests/       — lang_tests/ + kernel/ + conformance/
docs/        — Plans + status snapshots + design notes
scripts/     — test runners + codebase-dump utilities
```

### `runtime/`

**~10,500 LOC of Rust.** The bootstrap compiler. Reads `.ev` files,
lexes/parses/translates to SMT-LIB or runs Z3 directly. Three
subcommands:

- `evident sample <file> <claim>` — sat-check via Z3, print bindings
- `evident emit <file> <claim>`   — translate to SMT-LIB
- `evident run <file> <claim>`    — emit + invoke kernel binary

**Do not add features here.** Every new feature should be expressible
either in Evident (via stdlib/) or as a Build* sugar claim. If
neither works, file a proposal — don't grow the Rust surface.

The runtime's *role* is fixed: lex + parse + translate. Its *shape*
will shrink in iter 3+ as compiler stages get rewritten in Evident.

### `kernel/`

**~750 LOC of Rust.** The minimal "trampoline + ffi + Z3 wrapper."
This stays Rust. End-state target: ~600 LOC.

Reads a `.smt2` file, runs Z3, walks the `effects` array, dispatches
each variant, loops until halt. The dispatch table is tiny — see the
"Effect enum floor" section below.

The kernel knows nothing about Evident syntax. It only knows SMT-LIB
+ libffi + the manifest header convention.

### `stdlib/`

**Evident library code.** The vocabulary userspace code is written
against. Currently:

- `combinatorics.ev` — Distinct, sorted, etc.
- `toposort.ev` — Topological sort claim
- `kernel.ev` — Effect/Result/LibArg enums + Build* sugar claims

**Adding a new system call = add a `BuildXyz` claim here.** Not a
kernel change, not a runtime change. The pattern:

```evident
claim BuildPrintln(s ∈ String, eff ∈ Effect)
    eff = LibCall("libc", "puts", ⟨ArgStr(s)⟩)
```

### `tests/`

Three test surfaces:

- `tests/lang_tests/*.ev` — claims with `sat_*` / `unsat_*` prefixes,
  driven via `evident sample --all --json`. Asserts language behavior.
- `tests/kernel/*.ev` — programs driven through `evident emit` +
  `kernel`, asserts stdout + exit code via `-- expect:` header comments.
- `tests/conformance/*.py` — black-box Python CLI tests.

## How to run tests

Single command, ~3 seconds:

```
./test.sh
```

Phase flags:
- `./test.sh --rust-only`     — skip Python phases
- `./test.sh --conformance`   — only conformance
- `./test.sh --lang`          — only lang tests
- `./test.sh --kernel`        — only kernel tests

**Always run `./test.sh` before declaring work done.**

## How to check Rust / kernel size

The shrink-toward-zero invariant means you should be checking these
regularly:

```bash
# Rust runtime LOC (target: trending toward 0 by end of iter 3+)
find runtime/src -name "*.rs" | xargs wc -l | tail -1

# Kernel LOC (target: ~600, stays stable forever)
find kernel/src -name "*.rs" | xargs wc -l | tail -1

# Evident stdlib LOC (target: grows as features migrate from Rust)
find stdlib -name "*.ev" | xargs wc -l | tail -1

# Token-equivalent for context budgeting:
scripts/dump-codebase.sh runtime/src | wc -w   # words ≈ 0.75× tokens
scripts/dump-codebase.sh kernel/src  | wc -w
```

The dump-codebase.sh script writes one markdown blob suitable for
feeding to a fresh agent (with comment-stripping by default; set
`STRIP_COMMENTS=0` to keep them).

When the Rust runtime LOC drops, something good happened. When it
grows, justify it in the commit message.

## Working in background sessions

This file is the handoff document for sessions that don't have the
conversation history. A background session reading CLAUDE.md should
have enough to:

1. Know what to modify (Evident, kernel rarely, runtime almost never)
2. Know how to test changes (`./test.sh`)
3. Know the language spec well enough to write Evident code
4. Know the kernel's contract well enough to write programs targeting it

**For multi-step or risky work, prefer spawning a background agent
over doing the work in the main conversation.** Use the Agent tool
with the dump-codebase.sh output as context.

When spawning an agent:
- Pass the relevant slice of the codebase via dump-codebase.sh
- Give it CLAUDE.md as the invariants doc
- Specify what it CAN and CANNOT modify
- Cap its response length

## Language spec — Evident

### Schema keywords

Four keywords produce the same AST node (`SchemaDecl`):

| Keyword | Use |
|---|---|
| `type` | Defines a record / nominal value type (a noun) |
| `claim` | A predicate / constraint / property (a verb-like assertion) |
| `schema` | Synonym for `type`. Avoid in new code |
| `fsm` | Currently a synonym for `claim`. Reserved for future use |

**`sat_*` / `unsat_*` test claims** are written as `claim`.

### Composition mechanisms

Seven ways to compose schemas. Use the shortest form that works:

| Form | Meaning |
|---|---|
| `variable ∈ TypeName` | Declare a typed variable; fields/invariants become accessible |
| `..ClaimName` | Inline the claim's body (names-match) |
| `ClaimName` (bare) | Inline via names-match (synonym; resolved at translation) |
| `ClaimName(slot ↦ value)` | Inline with explicit slot binding |
| `(a, b) ∈ ClaimName` | Inline with positional binding to first-line params |
| `cond ⇒ ClaimName` | Conditional inline (constraints wrapped in `cond ⇒ …`) |
| `recv.subclaim(args)` | Subclaim dispatch with receiver-prefix |
| `subclaim Name` | Nested claim registered as a top-level schema |

### Chained membership

```evident
x ∈ Int = 5            -- declare + pin
x ∈ Int < 10           -- declare + upper bound
0 < x ∈ Int < 10       -- declare + range
a, b, c ∈ Int < 5      -- multi-name (3 decls, each bounded)
```

### Records & lift forms

Define short records once:

```evident
type IVec2(x, y ∈ Int)
type Color(r, g, b ∈ Nat)
```

Then four lifts work automatically:
1. **Componentwise comparison** — `a < b`, `a = b`, `lo ≤ x ≤ hi`
2. **Arithmetic broadcast** — `c = a - b`
3. **Type-use pins** — `pos ∈ IVec2(380, 280)` or `pos ∈ IVec2(x ↦ 1)`
4. **Record literals in expressions** — `state.pos = IVec2(0, 0)`

### Seq

```evident
items ∈ Seq(Int) = ⟨1, 2, 3⟩       -- literal
xs ∈ Seq(Int) = a ++ b ++ ⟨c⟩       -- `++` flattens at load time
#items = 3                          -- cardinality (also: string length)
∀ x ∈ items : x > 0                 -- element iteration
∀ (cur, nxt) ∈ coindexed(a, b) : …  -- parallel zip
∀ (a, b) ∈ edges(seq) : …           -- consecutive pairs
```

### Enums

```evident
enum Color = Red | Green | Blue
enum Result = Ok(Int) | Err(String)
enum LL = Nil | Cons(Int, LL)
enum A = X(B) ; enum B = Y(A)       -- forward refs + mutual recursion
```

Variant names are globally unique. Payload accessors are auto-named
`<Variant>__f<N>` (unique within their enum).

### Match & matches

```evident
n = match e
    Ok(v)  ⇒ v
    Err(_) ⇒ 0

is_ok = e matches Ok(_)              -- Bool recognizer
```

### Generics

```evident
type Edge<T>(from, to ∈ T)
claim Toposort<T>
    n ∈ Nat
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)

es ∈ Seq(Edge<Rect>)
Toposort<Rect>(n ↦ 4, items ↦ rects, …)
```

Type-parameter names are capitalized. Explicit type args only — no
inference at call sites yet.

### Boolean & precedence footguns

- `true` / `false` are lowercase. `True` parses as an unbound name
  — the constraint silently drops.
- `⇒` binds tighter than `∧`. Wrap compound consequents:
  `A ⇒ (B ∧ C)`.
- `=` binds tighter than `∧` / `∨` / comparisons. Wrap boolean
  assignments: `flag = (x < 5 ∧ y > 0)`.

### Type inference

The runtime recovers types from RHS expressions:

```evident
ok = (x > 0)                        -- Bool from comparison
mid = (n > 0 ? n : 0 - n)           -- Int from ternary arms
sky = Color(80, 160, 220)           -- Color from ctor
target = _world.pos                  -- IVec2 from field type
```

What stays explicit: top-level literal pins (`x = 5` needs
`x ∈ Int = 5`), `type` body memberships.

### Idioms to avoid

- **Parallel Seqs.** If `from ∈ Seq(Int)` and `to ∈ Seq(Int)` are
  "supposed to align," use a record type:
  `type Edge(from, to ∈ Int)` + `edges ∈ Seq(Edge)`. Z3 will silently
  fill in unconstrained values — misaligned parallel Seqs become
  silent wrong-answer bugs.
- **Indices in interfaces.** If a claim's input/output uses `Int`
  indices to identify "which item," you're leaking an implementation
  choice. Domain types in, domain types out.
- **Stacked ternaries.** Three ternaries hard-coding the same
  constant = an entity system. Define the entities and let
  constraints do the work.
- **Range-of-indices quantifiers.** Prefer `∀ x ∈ seq : …` over
  `∀ i ∈ {0..#seq - 1} : … seq[i] …`.

## Kernel runtime spec

### CLI

```
evident sample <file> <claim> [--json] [--given k=v ...]    # solve, no I/O
evident sample <file> --all [--json]                        # sat-check every claim
evident emit   <file> <claim> [-o out.smt2]                 # translate to SMT-LIB
evident run    <file> <claim>                               # emit + exec kernel

kernel <file.smt2>                                          # run a compiled program
```

### Effect enum floor

```evident
enum Effect =
    ReadLine
    ReadFile(String)
    WriteFile(String, String)
    LibCall(String, String, Seq(LibArg))
    Exit(Int)
```

| Variant | Status |
|---|---|
| `LibCall(lib, fn, args)` | Kernel-native — libffi dispatch |
| `Exit(code)` | Kernel-native — process exit (short-circuit) |
| `ReadFile(path)` | Kernel built-in (uses `std::fs`) |
| `WriteFile(path, contents)` | Kernel built-in (uses `std::fs`) |
| `ReadLine` | Kernel built-in (uses stdin readline) |

The three remaining built-ins (`ReadFile`/`WriteFile`/`ReadLine`)
need richer libffi shapes to be demoted — buffer types, fd handles.
Until those land, they stay as kernel built-ins.

**Sugar claims in stdlib/kernel.ev** wrap LibCall for common ops:
`BuildPrintln`, `BuildPrint`, `BuildTime`. Adding more = no kernel
change.

### Halt conditions, in priority

1. `Exit(code)` emitted → exit `code`. Mid-batch is fine; later
   effects in the same Seq are dropped.
2. `state_next == state` with no Exit → exit 1 ("stuck").
3. UNSAT on a tick → exit 2.
4. Internal error (Z3 crash, libffi crash, OOM) → exit 3.

Tick limit: 100,000.

### last_results carry

Each effect produces a `Result`:
- `NoResult` — void-returning effects (puts, write, …)
- `IntResult(Int)` — libcall returning int
- `StringResult(String)` — ReadLine line, ReadFile contents
- `RealResult(Real)` — libcall returning double
- `EofResult` — ReadLine at EOF
- `ErrorResult(String)` — any effect-level failure

The kernel collects these into a `Vec<Result>` and asserts them as
`last_results` on the next tick. The FSM pattern-matches:

```evident
contents ∈ String = match last_results[0]
    StringResult(s) ⇒ s
    _ ⇒ "<error>"
```

### State carry across ticks

State fields = top-level memberships of primitive type
(`Int`/`Bool`/`Real`/`String`), excluding `effects`, `last_results`,
`is_first_tick`, and `_<name>` carry-overs.

The kernel:
1. Reads each state field from the model after solve.
2. On next tick, asserts `_<name> = <prev value>`.

The FSM body can use `_<name>` to reference the previous tick's
value (must be explicitly declared: `_count ∈ Int`).

### is_first_tick

`emit` auto-injects `is_first_tick ∈ Bool` if the user doesn't
declare it. The kernel asserts:
- `is_first_tick` on tick 0
- `(not is_first_tick)` on subsequent ticks

### Single-writer rule for `effects`

`effects = ⟨a⟩ ∧ effects = ⟨b⟩` is UNSAT. `evident emit` enforces:

- At most one *unconditional* `effects = <expr>` constraint
- Multiple *guarded* `cond ⇒ effects = <expr>` constraints are
  allowed (user-responsible mutual exclusion)

Multi-writer composition uses `++`:

```evident
effects = effects_log ++ effects_work ++ effects_exit
```

### Manifest header

Every kernel-runnable `.smt2` starts with:

```
;; manifest: state-fields = <name>:<Type> …
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = <N>
```

Required + in order. The kernel parses this before invoking Z3.
See `docs/plans/kernel-input-spec.md` for the full contract.

## Patterns: how to add things

### A new system call

1. Find the libc (or other library) function.
2. Add a `BuildXyz(args, eff)` sugar claim to `stdlib/kernel.ev`
   that constructs the appropriate `LibCall(...)` value.
3. Done. No kernel change, no runtime change.

### A new test

- Language feature → claim under `tests/lang_tests/`.
- Behavior of a compiled program → fixture under `tests/kernel/`
  with `-- expect: stdout = "..." ; -- expect: exit = N` header.
- CLI surface → Python test under `tests/conformance/`.

### A new claim / type pattern

Just write Evident. No machinery to update.

### A new built-in effect (DISCOURAGED)

Stop. Almost everything should be a `LibCall`. If you're sure:
1. Add the variant to `Effect` in `stdlib/kernel.ev`.
2. Add a match arm in `kernel/src/tick.rs::dispatch_effect`.
3. Document why it can't be a LibCall in the relevant doc.

The "can't be a LibCall" bar is high. Buffer management, signal
handling, multi-step state machines — these might justify a built-in.
"It's faster" doesn't.

### A new language feature in the runtime (FORBIDDEN by invariant)

Don't. File a proposal in `docs/plans/` instead. Once iter 3+ starts,
runtime changes require explicit user approval.

## Style for Evident source

- Drop annotations the inference recovers.
- Default to no comments. Add one only when *why* isn't obvious.
- Record types over parallel Seqs.
- Element-form iteration over index ranges.
- A compact entry-point reads as wiring; logic lives in claims.

## Style for Rust source

(There should be very little Rust written in any session. But if
you must…)

- No new dependencies without justification.
- Match the existing layout under `runtime/src/`.
- Run `./test.sh` after every change.
- Commit with a clear message naming the iter you're in.

## When you're stuck

If a feature is missing, in priority order:

1. **Can you express it in Evident, in user code?** Then do that.
2. **Can a Build* sugar claim in stdlib/kernel.ev cover it?** Add it.
3. **Can `evident emit` handle it without runtime changes?**
   (Sometimes a Z3-side asserter does the trick.) Adjust emit.rs.
4. **Does the kernel need a new built-in?** Justify in the commit.
5. **Does the runtime need new translation?** Stop. File a proposal.

The further down this list you go, the higher the bar.
