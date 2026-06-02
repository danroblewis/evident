# Evident — read this first

## If you're being asked to orchestrate

If the human just asked you to "take over as coordinator" or
similar, **stop reading this file** and read
`docs/briefings/orchestrator.md` first. It contains the
hand-off prompt + the coordination pattern. Come back here
after.

## The goal in three sentences

**The deliverable of this project is `bootstrap/` deleted.** Today,
`bootstrap/runtime/` is ~10,500 lines of Rust that compile Evident
source files to SMT-LIB. We are transcribing that compiler into
Evident itself, in `compiler/*.ev`, so that the kernel can run the
compiler and the Rust can be removed. **Done = `bootstrap/` does
not exist; no Python lives under `tests/` or `scripts/`; the kernel
plus `compiler.smt2` are the entire system.**

If you find yourself thinking "let me improve / fix / refactor /
clean up something in `bootstrap/` or in the Python scripts" — stop.
That tree is reference material. Reference is read, not edited. We
edit Evident.

## The architecture, in one paragraph

The kernel (`kernel/`, ~880 lines of Rust) is the minimal native
runtime: trampoline + libffi + a Z3 wrapper. **It only knows how to
read an `.smt2` file and run it.** Evident source compiles to a Z3
model, which exports as SMT-LIB, which the kernel runs. **The
compiler is therefore just an Evident program that, when compiled
to `compiler.smt2`, takes another `.ev` file as input and emits the
corresponding `.smt2` as output.** Self-hosting in this project is
trivial in shape: once `compiler.smt2` exists, the kernel runs it
to compile every other Evident file, and `bootstrap/` has no role.

```
DELETION TARGET (the picture we are building):

  source.ev ─┐
             │
             ▼
  ┌────────────────────────────────┐
  │  kernel + compiler.smt2        │   ← reads source.ev, emits output.smt2
  └────────────────────────────────┘
             │
             ▼
       output.smt2 ─┐
                    │
                    ▼
  ┌────────────────────────────────┐
  │  kernel                        │   ← reads output.smt2, runs the program
  └────────────────────────────────┘
                    │
                    ▼
                  exit / stdout

  Nothing else exists. No Rust beyond kernel/. No Python.
  bootstrap/ has been deleted.
```

## Definition of done (mechanical, not aspirational)

The project is finished when all of these are true at once:

1. `ls bootstrap/` returns "No such file or directory."
2. `find scripts tests -name '*.py'` returns nothing.
3. `compiler.smt2` exists at the repo root and was produced by
   `kernel + a previous compiler.smt2 + a compiler.ev source file`
   (no bootstrap on the producing path).
4. `./test.sh` is green and references `bootstrap/` nowhere.
5. `scripts/check-deletable.sh` exits 0 with the message
   "BOOTSTRAP DELETABLE NOW" — and we've then actually deleted it.

If `scripts/check-deletable.sh` exits 1, the project is not done.
The script is the single source of truth for "are we there."

## Freeze rules (effective now)

| Tree                              | Status                          | What you may do                                                                                          |
| --------------------------------- | ------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `bootstrap/`                      | **FROZEN — reference material** | Read. Delete (when the replacement is verified). Nothing else. No edits, no bug fixes, no cleanups.       |
| `kernel/`                         | **Active construction; freeze applies when complete** | Edit freely for capabilities the kernel must have (Z3 lifecycle, FFI dispatch, functionizer, trampoline). Do NOT add language-runtime features that belong in `compiler/` or `stdlib/`. When the project is DONE (`bootstrap/` deleted, compiler self-hosted), this becomes a hard freeze. |
| `scripts/*.py`, `tests/**/*.py`   | **FROZEN — scheduled removal**  | Read. Delete (when replaced). No new lines, no new files. Replacements go in `scripts/*.sh` or `compiler/*.ev`. |
| `legacy-python/`, `legacy-rust/`  | **Scratch reference**           | Read during work sessions. Delete when the ideas they contain are implemented in the actual code (or proven not needed). Not part of the final state.                       |
| `scripts/*.sh`                    | **Transition-only growth**      | Only when Evident cannot yet express the glue. Mark with `# TODO: rewrite in Evident` header.            |
| `compiler/*.ev`                   | **GROW — this is the work**     | The self-hosted compiler lives here.                                                                     |
| `stdlib/*.ev`                     | **GROW — runtime library**      | Stable library code (Effect/Result enums, Build* sugar, combinatorics, toposort).                        |
| `tests/kernel/*.ev`               | **GROW**                        | Kernel-runnable test fixtures.                                                                           |
| `tests/conformance/features/*`    | **GROW**                        | New feature-spec conformance tests (implementation-agnostic). See `tests/conformance/features/README.md`. |

### Why "no Rust changes, no Python changes" is hard

Because subordinate sessions will be tempted to "just fix this one
small thing" in bootstrap/ to unblock their work. **Don't.** A
bootstrap bug is a signal to accelerate the replacement, not to
patch the past. If something in bootstrap/ is genuinely blocking
you, file a note in `docs/plans/` describing the block and stop. We
will route around it; we will not edit it.

A session that touches frozen code has failed regardless of whether
its tests pass. Reviewing sessions should reject the diff.

## The deletion path (the only way to make progress)

You eliminate frozen code by replacing it, not by editing it. The
sequence:

1. Pick a capability that bootstrap/ currently provides (e.g. "lex
   a `.ev` file's string literals", "translate an `EBinOp(OpPlus,
   …)` to `(+ …)`").
2. Implement it in `compiler/*.ev` (or `stdlib/*.ev` if it's
   library, not compiler).
3. Add a conformance test in `tests/conformance/features/` that
   defines the capability as an input/output spec.
4. Run the test against both implementations:
   - bootstrap: `IMPL=bootstrap tests/conformance/runner.sh ...`
   - self-hosted: `IMPL=selfhost ...` (uses kernel + the current
     `compiler.smt2`)
5. When both produce equivalent output for that test, the capability
   is "self-host ready." Mark it in `docs/plans/DELETION-CHECKLIST.md`.
6. When ALL features in `tests/conformance/features/` are self-host
   ready, the self-hosted compiler is feature-complete. We then:
   a. Compile `compiler/compiler.ev` one final time via bootstrap.
   b. Commit the resulting `compiler.smt2` to the repo root.
   c. Flip `test.sh` and `scripts/evident-self` to use `kernel +
      compiler.smt2`.
   d. Run `scripts/check-deletable.sh`. It should print
      "BOOTSTRAP DELETABLE NOW."
   e. `rm -rf bootstrap/`. Commit. Done.

That's the whole project. Every change to this repo should be a
step on that sequence. If it isn't, it's intermediate scaffolding
at best.

## Where the work currently is (one paragraph; not a roadmap)

`stdlib/lexer.ev`, `stdlib/parser.ev`, and `stdlib/translate_*.ev`
exist as per-pass demonstrations of how each AST shape maps to
SMT-LIB. **They do not yet compose into a working compiler.** Each
file handles its canonical shape one level deep and is exercised
by a fixture that hardcodes the input. No file reads a `.ev` from
disk via `ReadFile`; no file composes the full lex → parse →
translate pipeline; no conformance test compares output to
bootstrap. The current state of `scripts/check-deletable.sh`
output is in `STATE.md`. **The next work session's first action is
to run that script and read the blockers list.**

These files will be moved to `compiler/` as part of restructuring
so that their location matches their purpose (building the
self-hosted compiler), separating them from production stdlib
(`stdlib/kernel.ev`, `stdlib/combinatorics.ev`, `stdlib/toposort.ev`).

## How to brief a subordinate session

A subordinate session (`claude -p ...`) reads this file plus
`docs/briefings/foundation.md` plus its task spec. Its first
runtime action is `scripts/check-deletable.sh` so it sees the
current blockers state. It then works exclusively in
`compiler/*.ev`, `stdlib/*.ev`, `tests/**/*.ev`, or `scripts/*.sh`
(with the TODO header). If its diff touches any frozen path, the
result is rejected.

The coordinator pattern that spawns these sessions is documented in
`docs/briefings/README.md`.

---

# Reference: language and kernel specs

Below is the spec material. Above is the project's actual goal.

## What Evident is

A constraint programming language. Programs are collections of
constraints over named variables. A Z3 SMT solver finds satisfying
assignments. The central abstraction is `schema` (or `type` /
`claim`): a named set defined by membership conditions.

An Evident program:
1. Defines schemas and their members.
2. The compiler translates to SMT-LIB.
3. The kernel runs the SMT-LIB: solves, dispatches effects, repeats.

## Project tree

```
bootstrap/           — Rust compiler, FROZEN, scheduled for deletion.
                      Reference material only.
  runtime/           — The Rust crate. Produces the `evident` binary.

kernel/              — Trampoline + libffi + Z3 wrapper. The minimal
                      native runtime. ~880 LOC Rust. Stays Rust;
                      stays minimal.

compiler/            — The self-hosted Evident compiler. (Currently
                      empty / being assembled; the WIP pieces are
                      under `stdlib/` for historical reasons and
                      will move here.)

stdlib/              — Evident library code that user programs
                      depend on (Effect/Result enums, Build* sugar,
                      combinatorics, toposort, …). Stable.

tests/
  conformance/
    features/        — Implementation-agnostic feature specs.
                      Each runs against bootstrap and/or self-hosted
                      compiler; pass when output matches.
    runner.sh        — Drives the feature tests under IMPL=...
    *.py             — Legacy Python conformance tests, FROZEN,
                      scheduled for migration to features/.
  kernel/*.ev        — Kernel-runnable test fixtures (header
                      comments declare expected stdout + exit).
  lang_tests/*.ev    — Sample/sat-check tests for language behavior.

scripts/
  check-deletable.sh — Single source of truth for "are we done?"
                      Run from repo root. Exits 0 only when bootstrap
                      can be deleted.
  *.sh               — Build/test glue. Mark each with
                      `# TODO: rewrite in Evident`.
  *.py               — Legacy Python glue, FROZEN, scheduled removal.

STATE.md             — Snapshot of `check-deletable.sh` output.
                      Updated when the state changes; the brutal
                      truth, no prose.

docs/
  briefings/         — Subordinate-session briefings. foundation.md
                      is the universal one every session reads.
  plans/             — Forward-looking proposals + the deletion
                      checklist.
```

## Language spec — Evident

### Schema keywords

Four keywords produce the same AST node (`SchemaDecl`):

| Keyword  | Use                                                                   |
| -------- | --------------------------------------------------------------------- |
| `type`   | Defines a record / nominal value type (a noun).                       |
| `claim`  | A predicate / constraint / property (a verb-like assertion).          |
| `schema` | Synonym for `type`. Avoid in new code.                                |
| `fsm`    | Currently a synonym for `claim`. Reserved for FSM-specific semantics. |

`sat_*` / `unsat_*` test claims are written as `claim`.

### Composition mechanisms

Seven ways to compose schemas. Use the shortest form that works:

| Form                       | Meaning                                                                  |
| -------------------------- | ------------------------------------------------------------------------ |
| `variable ∈ TypeName`      | Declare a typed variable; fields/invariants become accessible.           |
| `..ClaimName`              | Inline the claim's body (names-match).                                   |
| `ClaimName` (bare)         | Inline via names-match (synonym; resolved at translation).               |
| `ClaimName(slot ↦ value)`  | Inline with explicit slot binding.                                       |
| `(a, b) ∈ ClaimName`       | Inline with positional binding to first-line params.                     |
| `cond ⇒ ClaimName`         | Conditional inline (constraints wrapped in `cond ⇒ …`).                  |
| `recv.subclaim(args)`      | Subclaim dispatch with receiver-prefix.                                  |
| `subclaim Name`            | Nested claim registered as a top-level schema.                           |

### Chained membership

```evident
x ∈ Int = 5            -- declare + pin
x ∈ Int < 10           -- declare + upper bound
0 < x ∈ Int < 10       -- declare + range
a, b, c ∈ Int < 5      -- multi-name (3 decls, each bounded)
```

### Records and lift forms

```evident
type IVec2(x, y ∈ Int)
type Color(r, g, b ∈ Nat)
```

Four lifts happen automatically:

1. Componentwise comparison: `a < b`, `a = b`, `lo ≤ x ≤ hi`.
2. Arithmetic broadcast: `c = a - b`.
3. Type-use pins: `pos ∈ IVec2(380, 280)` or `pos ∈ IVec2(x ↦ 1)`.
4. Record literals in expressions: `state.pos = IVec2(0, 0)`.

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

### Match and `matches`

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

Type-parameter names are capitalised. Explicit type args only.

### Boolean and precedence footguns

- `true` / `false` are lowercase. `True` parses as an unbound name
  — the constraint silently drops.
- `⇒` binds tighter than `∧`. Wrap compound consequents:
  `A ⇒ (B ∧ C)`.
- `=` binds tighter than `∧` / `∨` / comparisons. Wrap boolean
  assignments: `flag = (x < 5 ∧ y > 0)`.

### Idioms to avoid

- **Parallel Seqs.** If `from ∈ Seq(Int)` and `to ∈ Seq(Int)` are
  "supposed to align," use a record type: `type Edge(from, to ∈ Int)`
  + `edges ∈ Seq(Edge)`. Misaligned parallel Seqs become silent
  wrong-answer bugs.
- **Indices in interfaces.** If a claim's input/output uses `Int`
  indices to identify "which item," you're leaking an implementation
  choice. Domain types in, domain types out.
- **Range-of-indices quantifiers.** Prefer `∀ x ∈ seq : …` over
  `∀ i ∈ {0..#seq - 1} : … seq[i] …`.

## Kernel runtime spec

### CLI (bootstrap; will be replaced)

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

`LibCall` and `Exit` are kernel-native (libffi dispatch / process
exit). `ReadFile` / `WriteFile` / `ReadLine` stay as kernel
built-ins until libffi gains richer shapes.

Sugar claims in `stdlib/kernel.ev` wrap `LibCall` for common ops:
`BuildPrintln`, `BuildPrint`, `BuildTime`. Adding a system call =
add a `BuildXyz` claim. No kernel change.

### Halt conditions, in priority

1. `Exit(code)` emitted → exit `code`.
2. `state_next == state` with no Exit → exit 1 ("stuck").
3. UNSAT on a tick → exit 2.
4. Internal error → exit 3.

Tick limit: 100,000.

### State carry across ticks

Top-level memberships of primitive type (`Int`/`Bool`/`Real`/`String`),
excluding `effects`, `last_results`, `is_first_tick`, and `_<name>`
carry-overs, are read from the model after each solve and asserted
as `_<name> = <prev value>` on the next tick.

### is_first_tick

`emit` auto-injects `is_first_tick ∈ Bool` if the user doesn't
declare it. The kernel asserts:
- `is_first_tick` on tick 0
- `(not is_first_tick)` on subsequent ticks

### Single-writer rule for `effects`

`effects = ⟨a⟩ ∧ effects = ⟨b⟩` is UNSAT. `evident emit` enforces:
- At most one *unconditional* `effects = <expr>` constraint.
- Multiple *guarded* `cond ⇒ effects = <expr>` constraints are
  allowed.

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

Required and in order. The kernel parses this before invoking Z3.
See `docs/plans/kernel-input-spec.md` for the full contract.

## Functionizer diagnostics (on by default)

The kernel prints a one-line summary at exit telling you what
functionized for the program you just ran. Looks like:

```
[functionizer] 5 total / 1 JIT / 1 interp / 2 residual; 0.8 ms total (0.0 ms func / 0.0 ms z3)
```

- `N total` = body assertions after simplify.
- `J JIT` = compiled to native code via Cranelift.
- `I interp` = extracted but interpreted (no JIT path for the shape).
- `R residual` = still goes to Z3 each tick.

Three env vars control the output:

- `EVIDENT_FUNCTIONIZE_STATS=verbose` — adds a per-step load
  report at startup with each step's shape category (`binop`,
  `ite`, `select`, `accessor`, `guarded-seq`, `seq-literal`,
  `unfunctionizable`). Use this when investigating why something
  fell through to Z3.
- `EVIDENT_FUNCTIONIZE_TRACE=1` — adds per-tick timing
  (`func ms / z3 ms / dispatch ms`). High-frequency; only useful
  for short investigations.
- `EVIDENT_FUNCTIONIZE_STATS=0` — silence everything.

The default is `Summary`. The line goes to stderr; it doesn't
pollute program stdout or affect test pass/fail.

When making implementation choices in `compiler/*.ev` or
`stdlib/*.ev`, you can read this output to confirm your shape
actually functionized. If you wrote what you thought was a
functionizable claim and `[functionizer]` shows it as residual,
the diagnostic is telling you the shape didn't extract — that's
a real signal worth investigating before pushing more code on
top.

## How to run tests

Today:

```
./test.sh
```

Phase flags: `--rust-only`, `--conformance`, `--lang`, `--kernel`.

Tomorrow (after the test refactor): `./test.sh` runs the same
phases but uses `tests/conformance/features/` under
`IMPL=bootstrap` until self-hosted is ready, then `IMPL=both` to
verify equivalence, then `IMPL=selfhost` only once bootstrap is
deletable.

## Style for Evident source

- Drop annotations the inference recovers.
- Default to no comments. Add one only when *why* isn't obvious.
- Record types over parallel Seqs.
- Element-form iteration over index ranges.
- A compact entry-point reads as wiring; logic lives in claims.

## Style for Rust source

You're not writing Rust. See "Freeze rules" above.

## When you're stuck

If a capability is missing, the only ladder is:

1. Can you express it in Evident, in user code? Do that.
2. Can a Build* sugar claim in `stdlib/kernel.ev` cover it? Add it.
3. Can `compiler/*.ev` handle it? Add the pass / extend it.
4. None of the above? File a note in `docs/plans/` describing the
   block. Do NOT edit Rust.

The further down this list you go, the higher the bar for the
above ones to fail first.
