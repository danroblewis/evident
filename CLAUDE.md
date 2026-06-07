# Evident — read this first

## State: post-bootstrap-deletion (commit 76dc491)

`bootstrap/`, `legacy-rust/`, and `legacy-python/` are gone. The
project is now self-hosted on `kernel + compiler.smt2`. The
producing path has zero Rust dependencies outside `kernel/` and
zero Python anywhere.

```
  source.ev ──→ kernel + compiler.smt2 ──→ output.smt2 ──→ kernel ──→ exit / stdout
```

That is the whole system. The kernel is ~880 lines of Rust
(trampoline + libffi + Z3 wrapper). `compiler.smt2` is ~2 MB of
SMT-LIB that the kernel parses and runs to translate `.ev` source
into more `.smt2`.

## What's next

Four phases, documented in `docs/plans/post-cutover-roadmap.md`:

1. **Wave 5a** — Z3 wrapper in Evident (FFI to libz3). Plan in
   `docs/plans/wave-5a-z3-in-evident.md`.
2. **Wave 5b** — Trampoline + libffi in Evident. Plan in
   `docs/plans/wave-5b-trampoline-ffi-in-evident.md`.
3. **Wave 5c** — Functionizer in Evident. Plan in
   `docs/plans/wave-5c-functionizer-in-evident.md`.
4. **Wave 5d** — AOT functionizer binary cache. Plan in
   `docs/plans/wave-5d-aot-binary-cache.md`.

Recommended order is 5a → 5b → 5c → 5d, with phases gated on each
other's named cross-wave blockers (see the roadmap).

## Tree layout

| Tree                              | Status                          | What you may do                                                                                          |
| --------------------------------- | ------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `kernel/`                         | **Active construction; freeze applies when complete** | Edit for kernel capabilities (Z3 lifecycle, FFI dispatch, functionizer, trampoline). Do NOT add language-runtime features that belong in `compiler/` or `stdlib/`. Targeted for shrinkage in waves 5a–5c. |
| `compiler/*.ev`                   | **GROW — the self-hosted compiler**     | This is where compiler.smt2's source lives. Edits must be paired with a rebuild via `scripts/build-compiler-smt2.sh` once the in-Evident build path lands; until then, `compiler.smt2` is the frozen artifact. |
| `stdlib/*.ev`                     | **GROW — runtime library**      | Stable library code (Effect/Result enums, Build* sugar, combinatorics, toposort).                        |
| `tests/kernel/*.ev`               | **GROW**                        | Kernel-runnable test fixtures.                                                                           |
| `tests/conformance/features/*`    | **GROW**                        | Implementation-agnostic conformance tests.                                                                |
| `tests/seam/*.ev`                 | **GROW**                        | Regression fixtures for the self-hosted path (Phase 6 of test.sh).                                       |
| `scripts/*.sh`                    | **Transition-only growth**      | Only when Evident cannot yet express the glue. Mark each with `# TODO: rewrite in Evident` header.       |

## Editing the self-hosted compiler

`compiler/*.ev` is the source. `compiler.smt2` at the repo root is
the compiled artifact the kernel runs. After editing `compiler/*.ev`,
the artifact must be rebuilt — and right now we have no
self-host-on-itself build path. Source edits are valid (the
language doesn't go away), but they don't take effect on tests
until `compiler.smt2` is rebuilt by a tool we don't yet have. The
wave-5 plans are the path to closing that loop (recognizer + codegen
in `compiler/*.ev` → AOT to `compiler.smt2`).

Until then, treat `compiler.smt2` as a checked-in binary artifact
and `compiler/*.ev` as its reference source.

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
kernel/              — Trampoline + libffi + Z3 wrapper. ~880 LOC
                      Rust. Targeted for shrinkage in waves 5a–5c.

compiler/            — The self-hosted Evident compiler source.
                      Compiled to compiler.smt2 (committed artifact).

stdlib/              — Evident library code (Effect/Result enums,
                      Build* sugar, combinatorics, toposort, …).

tests/
  conformance/
    features/        — Implementation-agnostic feature specs.
                      runner.sh drives them under IMPL=selfhost.
  kernel/*.ev        — Kernel-runnable test fixtures (header
                      comments declare expected stdout + exit).
  lang_tests/*.ev    — Sample/sat-check tests for language behavior.
  seam/*.ev          — Regression fixtures for the self-hosted path.

scripts/
  evident-self       — CLI; `bin` returns the kernel+compiler.smt2
                      wrapper used by every test/bench script.
  run-{kernel,lang,seam,sample}-*.sh — test phase drivers.
  flatten-evident.sh — Import resolver (compiler.smt2 doesn't do
                      imports); pipe its output to kernel+compiler.smt2.
  mem-cap.sh         — Polling RSS watchdog (macOS doesn't honor
                      RLIMIT_AS). Wired into the seam wrapper.
  cc-wrapper.sh      — Linker shim that patches the kernel binary's
                      libz3 install-name (see .cargo/config.toml).

compiler.smt2        — The compiled self-hosted compiler. Built by
sample.smt2          — The compiled sample/sat-check driver. Both
                      are committed artifacts; rebuilding them is
                      blocked on the wave-5 plan.

STATE.md             — Current state of the project, in prose.
docs/plans/          — Forward-looking proposals + wave-5 plans.
docs/briefings/      — Subordinate-session briefings.
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

### CLI

The self-hosted CLI is `scripts/evident-self`. `bin` prints an
ephemeral wrapper that runs `kernel + compiler.smt2`; every test
and bench script resolves its `evident` through that path:

```
evident-self emit   <file.ev> <claim> [-o out.smt2]    # translate to SMT-LIB
evident-self sample <file.ev> [--all] [--json]         # sat-check claims
evident-self bin                                        # path to the wrapper

kernel <file.smt2>                                      # run a compiled program
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

Phases 1+2 build and test the kernel. Phase 3 runs conformance
under `IMPL=selfhost`. Phases 4+5 drive lang_tests and kernel
fixtures via the seam wrapper. Phase 6 runs the seam smoke
regression. There is no `IMPL=bootstrap` anymore.

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

## Long-running commands

Run anything expected to take more than ~30 seconds (cargo build, test
suites, kernel/solver runs on .smt2 workloads) **in the background**
(`run_in_background: true`), then poll its output and continue other work
meanwhile. The operator steers this session remotely mid-turn; a foreground
command blocks their messages from being read until it finishes.
