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
| `fsm`    | A `claim` whose referenced `_x` prev-tick carries are auto-synthesized (its FSM-specific semantics; see below). Use for stateful tick-machines. |

`sat_*` / `unsat_*` test claims are written as `claim`.

`fsm` is no longer a bare synonym for `claim`. The pre-oracle transform
`scripts/expand-fsm-autocarry.sh` rewrites `fsm Name` → `claim Name` and,
for every bare field `x ∈ T` whose `_x` is referenced in the body, inserts
a `_x ∈ base(T)` carry-dual — so a carry-bearing machine is written `fsm`
with no hand-written `_<name>` declarations. A plain `claim` (no autocarry)
still needs explicit `_x` decls. Use `fsm` for anything that carries state
across ticks; reserve `claim` for pure predicates/helpers. `fsm`
composition also threads carries into the parent (slot-bind injects
`_x ↦ _y`; `..`-lift and nested forms work) — see
`docs/plans/fsm-composition.md`.

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

### Type invariants — a type is its constraints, not its fields

The header line `type T(a, b ∈ …)` only names the fields. The **body**
is where the type earns its keep: a type is "a named set defined by
membership conditions," so put the invariants that **bind the fields'
relationships** in the body. A bodyless `type T(a, b)` is an anemic
tuple — avoid it. When you write `x ∈ T`, the oracle instantiates the
body over `x`'s fields, and the kernel **re-checks it every tick**: a
violation on carried state surfaces as a loud `UNSAT` (exit 2) instead
of a silent wrong answer. This is the single highest-leverage thing a
type does in this language.

```evident
-- GOOD: the body binds count to cap — a memory-safety contract.
-- `cap` exists precisely so the cursor-in-bounds relationship is
-- expressible; a good type may need a field to state its invariant.
type FtiBuffer(base ∈ Int, count ∈ Int, cap ∈ Int)
    base ≥ 0           -- a real calloc'd address, never negative
    cap  ≥ 0           -- the region holds `cap` slots
    0 ≤ count ≤ cap    -- the write cursor never passes the last slot
```

An append that would drive `count` past `cap` makes the tick UNSAT —
the overrun halts the kernel instead of corrupting memory.

**Lifecycle types: conditional invariants are fine — disequality is not.**
A type that is carried state passes through an uninitialized boot window
where its handles are legitimately `0`. A universal `handle ≠ 0` would
falsely fail tick 0. The natural fix is a **conditional** invariant,
vacuously true during init and binding the relationship once live. Write
the liveness test with a **convex** comparison (`> 0`), not `≠ 0`:

```evident
-- "if the solver is live, so are its context and config."
-- Z3 handles are positive pointers, so `> 0` means "live" — and it
-- functionizes, where `≠ 0` would not (see the caveat).
type Z3SolverCtx(cfg ∈ Int, ctx ∈ Int, sol ∈ Int)
    cfg ≥ 0
    ctx ≥ 0
    sol ≥ 0
    sol > 0 ⇒ (ctx > 0 ∧ cfg > 0)
```

> **⚠ PERF CAVEAT (hard-won, 2026-06-08).** The cost is about the
> **operator, not the `⇒`**. A disequality (`≠`) is *non-convex* — Z3
> reads `x ≠ 0` as `x < 0 ∨ x > 0` and must case-split. On carried state
> that the model references heavily (the compiler's Z3 handles), that
> case-splitting compounds every tick across thousands of ticks and
> **explodes**: the lifecycle invariant written with `≠ 0` took
> fixture-001 from **19 s to a >30-min timeout**. The *same implication*
> written with `> 0` stays fully functionized (`0.0 ms z3`, 20 s). Convex
> comparisons (`> ≥ ≤ < =`) and the implication itself are cheap; the `≠`
> was the trap. **Rule: never put `≠` on hot carried state — if you mean
> "nonzero" and the sign is known, write `> 0` / `< 0`.** Guard it with
> `scripts/functionization-gate.sh` (asserts the compiler + the FTI perf
> fixtures stay near-zero `ms z3`); check the `[functionizer]` line
> yourself (`0.0 ms z3` = good, nonzero = a constraint fell to Z3).

Rules of thumb for writing a type body:

- State what must **always** be true of the fields — including during
  the boot window. If a property only holds once initialized, guard it
  (`live ⇒ …`) with a convex test (`sol > 0 ⇒ …`).
- Prefer invariants that **relate** fields (`count ≤ cap`, `sol > 0 ⇒
  ctx > 0`) — the relationship *is* the abstraction.
- **No `≠` on hot carried state.** It is the one operator that reliably
  falls off the fast path. Convex comparisons are safe.
- Mind the footguns: `⇒` binds tighter than `∧`, so wrap consequents
  `A ⇒ (B ∧ C)`; `=` binds tighter than comparisons, so wrap boolean
  assignments. Chained membership (`0 ≤ count ≤ cap`) works in a body.
- Adding an invariant is a **behavior change**, not a refactor — gate it
  on conformance + the type's carry/violation unit tests + the
  functionization gate, never the byte-identical emit gate. A conformance
  *failure* means the invariant is actually false somewhere (real
  signal); a conformance/gate *timeout* means it fell off the
  functionizer (the `≠` trap) — investigate before weakening or dropping.

### Seq

```evident
items ∈ Seq(Int) = ⟨1, 2, 3⟩       -- literal
xs ∈ Seq(Int) = a ++ b ++ ⟨c⟩       -- `++` flattens at load time
#items = 3                          -- cardinality (also: string length)
∀ x ∈ items : x > 0                 -- element iteration
∀ (cur, nxt) ∈ coindexed(a, b) : …  -- parallel zip
∀ (a, b) ∈ edges(seq) : …           -- consecutive pairs
```

**The registry pattern — allocate by position, everything else by key.**
A carried registry (a bounded `Seq` of records filled over time) has
exactly one legitimately positional operation: **allocation** — placing
a new entry means choosing one slot among identical empties, which is
what a cursor is. Every other operation keys on a unique field:

```evident
∀ k ∈ {0..5} : xs[k].name = (… (alloc ∧ _cur = k) ? new_nm …)   -- alloc: positional, honest
∀ e ∈ xs : e.val = (… (upd ∧ _e.name = key) ? v : _e.val)       -- update: BY KEY, no index
∀ e ∈ xs : ((e.name = key) ⇒ (out = e.val))                     -- read: keyed projection
```

`_e` is the element's prev-tick carry dual (the fsm `_x` convention
applied to the bound element). Never store an index as FSM state to
identify an entry — store the key (`setvar_cur_name ∈ String`, not
`setvar_cur ∈ Int`); an index-valued lookup (`idx = (name = xs[0].name ? 0
: …)`) is the index-in-interface idiom — write a keyed projection
instead. Each field needs exactly ONE covering write: an `++`-append
covers every field of its slot, so a seq with later field updates must
allocate per-field, never by append. `∀ k ∈ {0..N-1}` survives only
where the position is the meaning: allocation cursors, wire positions
passed to claims (`i ↦ k`), positional-parameter slots, order-sensitive
folds.

**Bound it and everything is fast.** The Z3 sequence theory is
semi-decidable only when a `Seq` is *unbounded*. Add a literal length
bound (`#xs ≤ N`) and every construction — index, `∀`/`∃`, sortedness,
prefix/suffix/contains/extract, the overlapping-chain ordering merge —
lowers to cheap, decidable Array+len / bounded-quantifier form. There is
no slow *operation*; there is only "is it bounded." See
`docs/seq-bounded-catalog.md` (a verified support matrix) and the
`tests/seq/` regression suite (39 Evident fixtures + 18 Z3 checks).

> **⚠ FOOTGUN: Seq membership `x ∈ xs` is SILENTLY DROPPED.** The frozen
> oracle cannot translate it — `x` never reaches the SMT, the constraint
> vanishes, and the claim goes *vacuously SAT* with NO error (exit 0).
> Use `∃ i ∈ {0..#xs-1} : xs[i] = x` instead. (`x ∈ Set(T)` is fine — the
> drop is Seq-specific.) `scripts/lint-seq-membership.sh <flat.ev>` flags
> the bad form loudly. Record-field access on a Seq element (`e.from` in
> `∀ e ∈ edges`) is also oracle-dropped — BUT on a **bounded** Seq
> (`#xs ≤ N`, registered by `scripts/lower-bounded-seq.sh`) the element
> forms `∀ e ∈ xs : …e.f…` and `(∃ e ∈ xs : …e.f…)` are lowered
> pre-oracle and work fine; they are the preferred surface. Only an
> UNBOUNDED Seq still needs `coindexed` parallel `Seq`s.

> **⚠ PERF TRAP: outputs must be COVERED, never implication-defined**
> (measured 2026-06-09). Defining a value by guarded pins —
> `∀ k : ((xs[k].name = key) ⇒ (out = xs[k].val))` plus a `¬∃ ⇒ default`
> — reads beautifully but emits *bare disjunctive constraints*: the
> functionizer extracts per-output assignments and cannot extract a bare
> `(A ∨ B)`, so every such line goes Z3-residual and the hot loop dies
> (fixture-001: 19 s → >300 s timeout with seven of them). The COVERED
> form of a keyed projection is the ternary select chain
> `out = (xs[0].name = key ? xs[0].val : … : default)` — same semantics,
> functionizes. ∀-instantiated *equality writes*
> (`∀ k : xs[k].f = (…ternary…)`) are assignments and are fine. Rule:
> every output variable needs one covering `=` assignment; `⇒`/`∨` may
> only appear *inside* its right-hand expression, never as the thing
> that defines it. **Exception — the bounded-Seq pair form is safe to
> write:** on a transform-lowered Seq, `scripts/lower-bounded-seq.sh`
> recognizes the `∀`-pin + `¬∃`-default PAIR and lowers it to the
> covered chain itself (first-match-wins; keys must be unique), so the
> pretty surface stays. Preferred (element form, occupied slots only):
>
> ```evident
> ∀ e ∈ xs : ((e.name = key) ⇒ (out = e.val))
> (¬(∃ e ∈ xs : e.name = key)) ⇒ (out = default)
> ```
>
> The index form (`∀ k ∈ {0..N-1} : ((xs[k].name = key) ⇒ …` with a
> `{0..N-1}` or `{0..#xs-1}` default) also lowers, covering ALL slots
> unguarded. A LONE pin or default still emits the bare form — the
> trap applies in full.

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

**Record-typed fields carry too.** A carried `r ∈ T` (record `type`)
declared alongside its `_r ∈ T` dual is flattened by the oracle into its
primitive field-consts (`r.f`/`_r.f`), which carry by the rule above — so
records are usable as evolving FSM state, not just transient values
(oracle fix, commit `c95710c`). Guardrails: **every field of a carried
record must have a covering assignment each tick** (else the kernel aborts
with `state var X not in model` — this also forbids wide "god-records"),
and **collections of records should be cons-list `enum`s, never `Seq`**
(`Seq` is ~250× slower on Z3 — see `docs/plans/blocked-cons-to-seq-perf.md`).
`Seq(T)` and cons-list `enum`s also carry. In an `fsm`, the autocarry
transform synthesizes the `_x`/`_r` duals automatically (above).

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

## Performance profiling — which constraints cost the most

**`scripts/perf-profile.sh <file.ev> <claim>`** answers "which
variables / sub-constraints cost the most to solve?" — use it
whenever a claim feels slow, before adding an invariant to hot
carried state, or to investigate a functionizer residual. It fuses
three signals the kernel + Z3 already expose:

- **marginal solve time per constraint** — the kernel's built-in
  band profiler (`EVIDENT_FUNCTIONIZE_TIMING`), reporting how much
  each constraint *added* to the tick-0 solve, with the variable it
  constrains. (Negative marginal = the constraint *sped* the solve.)
- **the constraint expression** — `EVIDENT_FUNCTIONIZE_DUMP` maps
  each band to the actual `(<= buf.count 2048)` text.
- **search-space size** — the tick-0 model through `z3 -st`:
  `decisions` / `conflicts` / `propagations` / **`rlimit-count`**
  (Z3's *deterministic* work counter — machine-independent).

Output is a ranked table of the costliest constraints plus the
model's global search-space stats. Flags:

- `--bisect` — binary-search the constraint set for the subset whose
  removal most cuts `rlimit-count`. The deterministic "what's blowing
  up the search" finder, in O(log n) Z3 runs. Use this on a big model
  (the compiler) where a one-shot ranking is too coarse.
- `--top N` / `--bands N` / `--reps N` — result count / band
  granularity / timing reps (min over reps cuts noise).

```
scripts/perf-profile.sh tests/compiler2_units/perf/fti_buffer_loop.ev main
scripts/perf-profile.sh compiler2/driver.ev driver_main --bisect
```

Companion gate: **`scripts/functionization-gate.sh`** asserts the
compiler and the FTI perf fixtures (`tests/compiler2_units/perf/`)
stay near-zero `ms z3` and under a wall ceiling — run it after any
change to a hot carried type's invariants. It is what catches the
`≠`-disequality class of regression (see the type-invariant perf
caveat above).

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

### The compiler2 conformance gate (the behavior gate for refactors)

`.goalpost/bin/run-conformance.sh` compiles + runs all 138 fixtures
through compiler2. A clean run is **~4 min** (16 jobs; 219s measured); it has a
**global 15-min wall cap** (`GP_GLOBAL_TIMEOUT`, default 900s) — if a
change makes fixtures slow (e.g. the `≠`-disequality trap), it **bails
and reaps the workers** instead of running for hours, and the artifact
is marked `bailed` (the measure goes red). `137/138` is the bar
(`123-subschema-shadowing-quantifier` is the one known failure).

**Workflow — fast gate per change, slow gate per batch.** Conformance is
the ~7-min long pole, so don't run it after every edit:

1. After each refactor step, run the **fast** gates: the affected
   isolation tests (`tests/compiler2_units/run.sh <module…>`) and
   `scripts/functionization-gate.sh` (~20s). These catch most breakage.
2. **Batch** several green-on-fast steps, then run conformance **once**.
3. If the batch fails or bails, **bisect**: re-apply the steps one at a
   time (or `git revert` them individually), running conformance on each,
   to isolate the culprit. Per-step commits make this trivial.

This amortizes the 7-min cost across a batch while keeping each step
independently revertible.

## Style for Evident source

- Drop annotations the inference recovers.
- Record types over parallel Seqs.
- Element-form iteration over index ranges.
- A compact entry-point reads as wiring; logic lives in claims.

### Comment rules

Default: **no comments**. Evident has no signatures, visibility, or
namespaces yet, so a few comment classes do work the language cannot —
keep ONLY these:

1. **Module contract headers** — `-- MODULE X` with
   `CONSUMES / PRODUCES / MAINTAINS`. Composition is names-match, so a
   module's interface is structurally invisible; the header IS the
   interface.
2. **Measured traps** — facts that cost an experiment to learn and whose
   violation *compiles fine but explodes later* (the `≠`-disequality
   perf cliff, latch ordering, frozen-oracle gaps). State the
   measurement and date. These are the highest-value lines in the repo.
3. **Cross-file encoding/wire facts** — fixed-width row formats, tag
   tables, "result lands at last_results[base+N]" API contracts:
   invariants that span files and live nowhere else.
4. **One-line section banners** (`-- ── X ──`) in long files —
   navigation; greppable.
5. **Test headers** — `-- entry:` / `-- expect:` (parsed by runners) plus
   a short purpose block; a fixture's header is its documentation.

Never write:
- a comment **restating the next line** (`cap ≥ 0 -- cap is at least 0`);
- **history/narration** ("demoted in iter 2.5", "session UU") — that's
  git's job, or `docs/`;
- **code examples inside prose comments** — text-level tools (lints,
  transforms) parse flattened source and a quoted `s ∈ Seq(Int)` in a
  comment has triggered real false positives;
- explanations of standard language semantics — that's this file's job.

The best comment is one converted into a **checked construct**: a
per-site bound-with-comment became `0 ≤ count ≤ cap` in the type body,
where the kernel enforces it every tick. When a fact becomes expressible
as an invariant or a test, move it there and delete the prose.

Comment-only edits to `compiler2/` must hold the byte-identical emit
gate (`scripts/driver-decomp-gate.sh`) — it proves the trim touched
nothing real.

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
