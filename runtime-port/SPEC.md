# Implementing an Evident Runtime — A Spec

## What this document is

A language-agnostic specification for what a working Evident runtime
must provide. The contract is `effect-run`: take an `.ev` source file,
load its FSMs, dispatch their effects until they halt. Anything else
(`evident query`, `evident test`, REPL, multi-fsm, etc.) is the same
machinery applied differently — if `effect-run` works, the rest is
glue.

The Rust runtime in this repo is one implementation. This doc
describes the *interface* every implementation must satisfy. Someone
should be able to read this and write their own Evident runtime in C,
Go, Python, Common Lisp — whatever the host language.

## External dependencies

Two external libraries are required. Everything else can be
implemented from scratch in the host language.

1. **Z3 SMT solver** — Evident's semantics ARE Z3. There is no way
   to "implement Evident without Z3" any more than you can implement
   SQL without a relational algebra. Treat Z3 as the language runtime
   for the constraint half of Evident.

   What the host needs from Z3: declare sorts and constants (Int,
   Bool, String, Array, Set, custom Datatype), build expressions
   over them, assert a list of expressions on a solver, push/pop the
   solver's assertion stack, check satisfiability, and read values
   out of a model on SAT. The official Z3 distribution exposes this
   via its C API; most language bindings (z3-rs, z3-py, z3-js, etc.)
   are thin wrappers over that.

2. **libffi (or equivalent)** — Evident programs reach the operating
   system via a single FFI primitive. With FFI, anything addressable
   via `dlopen` + `dlsym` is reachable from Evident. Without FFI, the
   host has to grow per-system bindings (a treadmill the architecture
   is designed to avoid). libffi is the standard but the host can use
   any equivalent that supports dynamic-library loading + run-time
   function calls with marshalled args.

No other libraries are required. No HTTP client, no graphics library,
no Unicode database — all of that becomes Evident library code on top
of FFI when needed.

## The pipeline

```
source bytes
    │
    │  lexer
    ▼
tokens
    │
    │  parser
    ▼
AST
    │
    │  load_file (handle imports, register schemas)
    ▼
schema table + enum registry
    │
    │  translator (per query / per FSM step)
    ▼
Z3 expressions on a solver
    │
    │  solver.check() + get_model()
    ▼
model bindings (typed Values)
    │
    │  effect dispatcher (orders + executes)
    ▼
EffectResults
    │
    │  fed back into next step as last_results
    ▼
(loop until halt)
```

Each arrow is a phase the host implements. The shape is the same
regardless of host language.

## Components

### 1. Lexer

Input: source bytes (UTF-8). Output: token stream.

Token categories:
- **Punctuation**: `()` `[]` `{}` `,` `:` `.` `=`
- **ASCII operators**: `<` `>` `<=` `>=` `!=` `+` `-` `*` `/` `?` `!`
- **Unicode operators** (each is one token, single source character):
  `∈` (in) `∉` (notin) `∀` (forall) `∃` (exists) `⇒` (implies)
  `⟸` (rev-implies) `≤` `≥` `≠` `⟨` `⟩` (seq-literal delims) `↦` (mapsto)
  `¬` `∧` `∨` `∪` `∩` `⊆` `⊂` `⊇` `⊃` `++` (concat) `#` (cardinality)
- **Keywords** (word tokens with special meaning): `type` `claim` `fsm`
  `schema` `subclaim` `enum` `external` `import` `match` `true` `false`
- **Literals**: integer (`42`, `-7`), real (`3.14`), string (`"..."`
  with `\n`, `\t`, `\\`, `\"` escapes), bool (`true` / `false`).
- **Identifiers**: ASCII letters + underscores + digits (digits not
  first). Identifiers can contain a literal `.` (dotted form) — see
  parser note below.
- **Indentation**: significant. Track indent depth per line; emit
  Indent(N) tokens at line starts. Two consecutive indented lines at
  the same depth belong to the same block.

The Rust lexer is `runtime/src/lexer.rs` — about 400 lines, mostly
straightforward token recognition. A reasonable implementation in any
language is in that range.

### 2. Parser

Input: tokens. Output: AST (the `Program` value with imports + a
schemas list).

Recursive-descent over the token stream. AST node types:

```
Program {
    imports:  list of strings
    schemas:  list of SchemaDecl
}

SchemaDecl {
    keyword:    one of {type, claim, fsm, schema, subclaim}
    name:       string
    external:   bool
    type_params: list of strings              -- for generics `<T, U>`
    param_count: int                          -- first-line param count
    body:        list of BodyItem
}

BodyItem = one of:
    Membership { name, type_name, pins }      -- "x ∈ Type [(pins)]"
    Constraint(Expr)                          -- any Bool-valued constraint
    Passthrough(string)                       -- "..ClaimName"
    SubclaimDecl(SchemaDecl)                  -- nested subclaim
    ClaimCall { name, mappings }              -- "ClaimName (slot ↦ value, …)"

Expr = one of:
    Identifier(string)                        -- variable or dotted access
    Int(i64) | Real(f64) | Bool(bool) | Str(string)
    SetLit(list of Expr)                      -- {a, b, c}
    SeqLit(list of Expr)                      -- ⟨a, b, c⟩
    Tuple(list of Expr)                       -- (a, b, c)
    Range(lo, hi)                             -- {lo..hi}
    InExpr(lhs, rhs)                          -- "x ∈ s"
    Forall(vars, range, body)                 -- "∀ x ∈ s : body"
    Exists(vars, range, body)
    Call(name, args)                          -- function/constructor call
    Cardinality(inner)                        -- "#expr"
    Index(seq, idx)                           -- "seq[i]"
    Field(receiver, name)                     -- "expr.field"
    Binary(op, lhs, rhs)                      -- arithmetic / comparison / logical
    Not(inner)
    Ternary(cond, then, else)                 -- "cond ? a : b"
    Match(scrutinee, arms)                    -- pattern match
    Matches(expr, pattern)                    -- "e matches Ctor(_, x)"

Pins = one of:
    None
    Named(list of (slot_name, value_expr))    -- "(a ↦ v1, b ↦ v2)"
    Positional(list of value_expr)            -- "(v1, v2)"
```

Important parsing details:

- **Dotted identifiers**: `a.b.c` (read-only, no Index in between)
  folds at parse time into a single `Identifier("a.b.c")`. `a[0].b`
  does NOT fold (the Index breaks the chain) and produces
  `Field(Index(Identifier("a"), Int(0)), "b")`.
- **First-line params** (`type IVec2(x, y ∈ Int)`) desugar to leading
  `Membership` items in the body. The `param_count` field records
  how many were declared in the parens.
- **Chained membership**: `x ∈ Int = 5` desugars to a `Membership`
  plus a `Constraint(Eq(Identifier("x"), Int(5)))`. Similarly for
  `x ∈ Int < 5`, range chains `0 < x ∈ Int < 5`, etc.
- **Method-style calls**: `win.draw_rect(args)` parses as
  `Call("win.draw_rect", args)`. The receiver/method split is the
  responsibility of dispatch (see translator).
- **Implies precedence**: `⇒` binds *tighter* than `∧` — unusual,
  but it's the language. `A ⇒ B ∧ C` parses as `(A ⇒ B) ∧ C`.
- **Indentation blocks**: implication consequents and `∀ : body`
  bodies can span multiple lines via indentation.

The Rust parser is ~1,900 lines and handles every quirk. A
greenfield parser will be in the same range.

### 3. Imports + schema registry

The runtime loads files in dependency order. Each `import "path"`
adds a file to the load queue; loaded files contribute their schemas
to a shared registry keyed by name. After loading completes:

- Every `SchemaDecl` (type, claim, fsm, …) is in one registry by name.
- Enum definitions (`SchemaDecl` with `keyword: enum` plus a
  variants list — see below) live in a separate enum registry by name
  and have their Z3 DatatypeSorts created up front.

Generic types like `type Edge<T>(from, to ∈ T)` need a
**monomorphization** pass: walk all schemas looking for uses of
`Edge<Rect>`, `Edge<Int>`, etc., and synthesize a concrete copy of
`Edge` for each instantiation. The concrete copies join the schema
registry under their synthesized names (e.g. `Edge<Rect>`). See
`docs/design/generics.md`.

Enum support requires staged datatype construction: types that
reference one another (`enum BodyItem` containing `Seq(BodyItem)`)
need their Z3 sorts created in topologically-sorted batches. The
host runtime must implement this batching — Z3 has a multi-datatype
constructor for it (`create_datatypes` / `mk_datatype_sort` etc.).

### 4. Translator: AST → Z3 constraints

This is the largest component. The translator's job: given a schema
and a `given` map (binding some names to values), build a list of
Z3 Bool constraints that capture the schema's semantics, plus an env
mapping each schema-local name to its Z3 representation.

Pipeline:

1. **Declare**: walk the schema body for `Membership` items. Each
   produces a Z3 const (or composite leaf-expansion) in env. Built-in
   types (Int, Nat, Pos, Bool, Real, String) get scalar consts.
   `Seq(T)` produces an Array(Int → T) + an Int length. `Set(T)`
   produces a Z3 Set. User types recurse: each Membership inside the
   type's body becomes another leaf at the dotted env key.

2. **Apply pinned ints + seq lengths**: a preprocessing pass walks
   the body for `n = literal_int` and `#seq = literal_int` patterns
   (and propagates: `n = 3 ; #seq = n` resolves). Pinned ints and
   pinned lengths get substituted into the env so quantifier
   unrolling sees literal bounds. See
   `runtime/src/translate/preprocess.rs`.

3. **Inline body items**: walk each body item and either declare more
   variables, recurse into a passthrough/ClaimCall/SubclaimDecl, or
   translate a constraint and assert it on the solver.

4. **Translate Constraint expressions**: each `Constraint(e)` becomes
   a Z3 `Bool`. The function `translate_bool(e, env, schemas)` is the
   main dispatcher; sub-helpers handle each Expr shape:

   - `Identifier(name)` → env lookup.
   - `Binary(Eq, a, b)` → try as Bool eq, Int eq, Real eq, String eq,
     enum eq, Seq-Lit eq (`s = ⟨1,2,3⟩`), Set-Lit eq, whole-Seq eq,
     record-equality lift (broadcasts componentwise), etc.
   - `Binary(Lt|Le|Gt|Ge|And|Or|Implies, …)` → translate operands +
     Z3 op.
   - `Binary(Add|Sub|Mul|Div, …)` → translates as Int/Real arithmetic.
   - `Forall(vars, range, body)` → unroll over the range (integer
     range, Seq elements, `coindexed(seqs)`, `edges(seq)`) and AND
     the per-iteration bodies. Range bounds must be statically
     resolvable (pinned ints / seq lengths).
   - `Exists` → unroll + OR.
   - `InExpr(x, s)` → set membership / range membership / set-literal
     OR.
   - `Cardinality(s)` → reads the Seq's `len` Int or Set's recorded
     candidate count.
   - `Index(seq, i)` → `arr.select(i)` with type coercion.
   - `Field(receiver, name)` → composite accessor application.
   - `Match(scr, arms)` → fold into a nested `ite` over each arm's
     tester.
   - `Call(name, args)` → enum constructor application, or a recurse
     for nested patterns like `Edge<Rect>(…)`.

5. **Subclaim / ClaimCall handling**: when the body refers to another
   claim by name (passthrough `..Foo`, ClaimCall `Foo(x ↦ a, …)`,
   subclaim invocation `recv.subclaim(…)`, or `(args) ∈ ClaimName`),
   the inline pass recursively translates the referenced claim's body
   in a fresh env with slot bindings applied. Each invocation gets a
   unique call-id suffix for its internal variables to avoid
   collisions across nested invocations.

6. **Constraint inheritance**: when a body item is `x ∈ TypeName`
   and `TypeName` is a user schema with its own body Constraints,
   those Constraints are inherited onto `x`. Bare references to
   `TypeName`'s fields rewrite to `x.field` (or for Seq elements,
   `Field(Index(name, i), field)`). Two layers: single-instance
   inheritance for `x ∈ T` and element-invariant inheritance for
   `x ∈ Seq(T)`.

7. **The dropped-constraint policy**: when `translate_bool` can't
   express a Constraint as a Z3 Bool (translator gap), the default
   behavior is to error out — better to refuse than silently emit
   an unsat-free program. The user opts into lenient mode with
   `EVIDENT_LENIENT=1` to demote drops to warnings.

The translator is the work. Expect 4–5x more code than the parser.
Most of the volume is the dozens of special-case translations
(SeqLit-eq into element-wise asserts, record-eq broadcast lifting,
Cons-chain enum SeqLit lowering, generic monomorphization, …). Each
is a small optimization or convenience pattern that lets users write
the language idiomatically; none is conceptually deep.

### 5. Solver loop

After translation, the host calls Z3:

```
solver.push()
for c in constraints: solver.assert(c)
result = solver.check()
if result == SAT:
    model = solver.get_model()
    bindings = extract(env, model)        -- env → Values
solver.pop()
return (result, bindings)
```

For long-running uses, cache the per-schema setup so subsequent
queries with the same structural shape (pinned ints / seq lengths)
reuse the asserted body. The Rust runtime has `build_cache` +
`run_cached` for this; an implementation can start without it and
add caching when profiling demands.

**Model extraction**: walk env, for each Var read its Z3 const's
model value, decode to a typed `Value`:

```
Value = one of:
    Int(i64) | Real(f64) | Bool(bool) | Str(string)
    SeqInt(list) | SeqBool(list) | SeqStr(list)
    SeqComposite(list of record-field-maps)
    SeqEnum(list of Value)
    SetInt(list) | SetBool(list) | SetStr(list)
    Enum { enum_name, variant, fields: list of Value }
    Composite(record-field-map)
```

For composite + enum extraction, recurse through the type's accessor
list, decoding each field's Z3 value the same way.

### 6. The FFI primitive

Evident's connection to the operating system. The runtime ships with
a single primitive operation: given a library path, a symbol name, a
type signature string, and a list of arguments, dynamically open the
library, look up the symbol, call it with the marshalled args, and
return a typed result.

Signature strings borrow libffi's notation. Returns first, then
parens-wrapped arg types:

- `v` void
- `i` 32-bit int  
- `l` 64-bit int
- `b` byte (u8)
- `f` 32-bit float
- `d` 64-bit float (Real)
- `p` pointer (opaque handle, see below)
- `s` C string (null-terminated)

Examples:
- `"i(piiii)"` — returns int, takes (pointer, int, int, int, int).
  Used for `SDL_SetRenderDrawColor`.
- `"p(s)"` — returns pointer, takes (string). Used for `dlopen`.
- `"v(p)"` — returns void, takes pointer. Used for `SDL_DestroyWindow`.

Arguments are tagged Values from the enum `FFIArg`:

```
ArgInt(i64)
ArgBool(bool)
ArgStr(string)
ArgReal(f64)
ArgHandle(i64)        -- u64 handle id from the registry; see below
ArgStrArr(list of strings)   -- for "const char **" parameters
ArgI32Buf(list of int)       -- for C functions that want an int32 buffer
ArgIntOut             -- "allocate a fresh int, pass &it, surface the
                       --  written-back value as the call's IntResult"
ArgPriorResult(idx)   -- substitutes another effect's result at marshal time
```

**Handle registry**: opaque C pointers (returned by `SDL_CreateWindow`,
`dlopen`, `malloc`, etc.) get registered into a runtime-side handle
map keyed by an integer (u64). Evident code sees the integer; the
runtime translates `ArgHandle(id)` back to the raw pointer at marshal
time. The integer is just a stable ID for Evident's value system —
pointers don't fit in Z3 sorts.

The primitive has multiple variants for different lifecycles:
- `FFIOpen(lib_path)` / `FFILookup(handle, symbol)` / `FFICall(symbol_handle, sig, args)` —
  three-step open/lookup/call for repeated calls.
- `LibCall(lib_path, symbol, sig, args)` — one-shot. The runtime
  caches the dlopen and dlsym internally; users don't manage handles.

`LibCall` is by far the more common form in practice.

### 7. Effects and the dispatch loop

Effects are how Evident programs interact with the outside world.
The `Effect` enum, declared in stdlib, has variants for built-in
operations (`Println(String)`, `Exit(Int)`, …) and for FFI primitives
(`LibCall(…)`, `FFIOpen(…)`, …). Programs emit Effects as bindings
in the solver model; the host walks those bindings and runs each
effect.

**Built-in effects** every host implementation must handle:
- `NoEffect` — no-op. The dispatcher drops these silently.
- `Print(String)` / `Println(String)` — write to stdout (the latter
  with newline). Result: `NoResult`.
- `Exit(Int)` — graceful halt. The dispatcher signals the scheduler
  to stop after the current tick's effects all dispatch, with the
  given int as the process exit code.
- `LibCall(lib_path, symbol, signature, args)` — FFI call as above.
  Result depends on the signature's return type: `i` → `IntResult`,
  `s` → `StringResult`, `p` → `HandleResult`, `v` → `NoResult`, etc.

Technically all of `Print` / `Println` could be FFI wrappers
(`LibCall("libc", "printf", "i(s)", …)` or similar). They stay
built-in for bootstrap convenience — a minimal "hello world" program
shouldn't need to know which libc the host uses.

Other Effect variants the stdlib defines (`ReadLine`, `Time`,
`MonotonicTime`, `ParseInt`, `IntToStr`, `ShellRun`, `Malloc`,
`ReadByte`, `WriteByte`, …) can be implemented either as host
built-ins or as Evident wrappers around `LibCall`. They're
documented in `stdlib/runtime.ev`'s Effect enum. A minimal runtime
can omit them; programs that don't use them won't notice.

**Effect ordering**: the model can contain multiple Effect-typed
bindings. Two ordering rules:

1. **Intra-Seq order** — when an Effect binding is part of a
   `Seq(Effect)` literal in the FSM body, the Seq's order is the
   dispatch order for those elements.
2. **Cross-Seq order** — bindings declared at the body level (not
   inside a Seq literal together) have no implicit order. The
   runtime picks a topological order from any user-declared
   `Seq(Effect)` ordering chains in the body, with random tie-break
   on unconstrained nodes. See
   `docs/design/effect-dispatcher-fsm.md`.

The default tie-break randomization is **load-bearing** for bug
discovery: programs that accidentally rely on a specific dispatch
order should surface that bug, not have it stabilized by the host.
Seed via `EVIDENT_DISPATCH_SEED` for reproducibility when debugging.

**Dispatch loop**:

```
loop:
    (state', effects) = solve(claim_main, state, last_results)
    if not SAT: error "FSM has no satisfying state — invariant broke"
    if exit_requested: break with exit_requested.code
    if effects empty and state' == state: break  -- fixpoint halt
    last_results = []
    for effect in topo_order(effects):
        result = dispatch(effect)
        last_results.append(result)
    state = state'
```

For single-FSM programs that declare an `effects` binding, this is
the whole loop. For multi-FSM programs, the scheduler is more
complex (see below).

### 8. Multi-FSM scheduler

When the loaded program has more than one claim matching the FSM
shape (state pair + `last_results ∈ Seq(Result)` + `effects ∈
Seq(Effect)`), the runtime instantiates each as an independent FSM
and coordinates them.

**Shared state via `world`**: if any FSM declares a `world ∈ World`
parameter where `World` is a record type, that record acts as shared
state. Per-FSM writes go into `world_next`; reads use `_world`
(previous tick) or `world` (this tick). The unify-world-syntax pass
rewrites the user's `world.X = expr` / `_world.X` shorthand into
the legacy writer pattern. **Multi-writer disjoint check**: each
`World` field can be written by at most one FSM per tick, enforced
at load time.

**Subscription-driven scheduling** (default; `EVIDENT_SCHEDULER=legacy`
for the older "tick everyone every iteration" behavior): an FSM ticks
only when one of its inputs changed —

- World fields it reads (auto-inferred — see
  `subscriptions::world_access_sets`).
- Its own previous state, if it emitted effects last tick.
- Bootstrap tick 0 — every FSM ticks once.

When no FSM is ready, the scheduler blocks on the async-event channel
or halts cleanly. Async events come from "plugins" — special FSMs
implemented in the host language that own resources (a frame timer,
the SIGINT handler, stdin reader, etc.) and push events into the
channel. These can be implemented in Evident using FFI; the host
needs to provide whatever underlying mechanisms (signals, threads,
polling) the Evident plugin code calls into.

**Halt**: program ends when `Effect::Exit(code)` is emitted, OR when
no FSM is scheduled in a tick AND no async event sources are open.

## What can be in Evident, what must be in the host

| Component | Host | Could be Evident | Notes |
|---|---|---|---|
| Lexer | YES | No | Bootstrap problem: source bytes → tokens. |
| Parser | YES | No¹ | Same. |
| AST → Z3 translator | YES | No | Calls Z3's API directly. |
| Z3 binding | YES | No | External dependency. |
| FFI primitive | YES | No | Calls libffi / dlopen / dlsym. |
| Effect dispatch loop | YES | No | Drives the FSM step cycle. |
| Multi-FSM scheduler | YES | No² | Drives ordering of independent FSMs. |
| Built-in effects (Print/Println/Exit) | YES³ | Maybe | Could be `LibCall`-wrapped; kept inline for bootstrap. |
| FFI effects (LibCall/FFICall/etc.) | YES | No | The primitive itself. |
| Other effects (Time/ParseInt/Malloc/etc.) | Either | YES | Pure Evident on top of LibCall. |
| FTI-typed resources (SDL_Window, etc.) | Either | YES | The plugin-as-writer pattern; stdlib already implements most. |
| Event sources (FrameTimer/SigintSource/StdinSource) | Either | YES⁴ | Currently in Rust; could be Evident FSMs using FFI. |
| Toposort dispatcher | YES⁵ | YES | Currently Rust; long-term goal is Evident, blocked on perf cold-start cost. |
| Subscription analysis | YES | No | Drives scheduling; pre-compute from AST. |
| Constraint inheritance | YES | No | Translator-internal AST rewrite. |
| Generics monomorphization | YES | No | Same. |

¹ A self-hosted parser is possible (see "Bootstrapping" below) but
deferred in this repo.

² A self-hosted dispatcher in Evident has been designed (see
`docs/design/effect-dispatcher-fsm.md`) but not implemented.

³ Only `Exit(code)` is truly load-bearing for halt; everything else
can be a stdlib `LibCall` wrapper. A minimal runtime can ship with
just `Exit` as built-in.

⁴ Each event source is naturally an FSM. The host provides the
underlying mechanism (signal handler, timer thread, file descriptor)
via FFI; the Evident program coordinates.

⁵ "Should be" Evident long-term; today it's Rust because per-frame
Z3 solving of the toposort costs ~ms vs Rust's μs.

## The bootstrap subset

Minimum Evident features the host runtime must support before it can
load the stdlib. Stdlib uses much of the language; the host must
support whatever stdlib does, recursively.

A reasonable boot-time subset:

**Lexer**: full. The lexer is small enough that there's no point in
shrinking it; the user types Unicode operators on day one.

**Parser**: needs to handle every shape that appears in
`stdlib/runtime.ev`:
- `import "path"`
- `enum Name = Variant1 | Variant2(Type) | …`
- `type Name(field ∈ Type, …)` with body Memberships and Constraints
- `external claim Name(args)` — external claim declarations for FFI
  wrappers
- `external fsm Name` — runtime-bridge contracts (e.g. SigintSource)

The other declaration forms (`fsm`, `subclaim`, generics) are needed
for user programs but stdlib itself doesn't use them. They can be
deferred if a user program isn't ready to need them.

**Translator**: needs to handle, at minimum:
- Variable declarations with primitive types
- Enum declarations + variant construction
- Equality + comparison + arithmetic
- `∀ x ∈ {0..N-1} : body` (integer range iteration)
- `Seq(Effect) = ⟨…⟩` literal assignment
- `match scrutinee\n  Variant(b) ⇒ body\n  _ ⇒ fallback`
- Subclaim invocation `recv.method(args)`
- `Effect::Exit(code)` and `Effect::LibCall(…)` recognition
- The FSM contract (state pair + effects + last_results)

That's enough to load stdlib's Effect/Result/FFIArg enums and the
runtime-bridge contracts.

**Effect dispatcher**: at minimum, `LibCall`, `Exit`, and one of
`Print` or `Println`. Everything else can come later as either a
host extension or a stdlib wrapper.

**FFI primitive**: at minimum, the one-shot `LibCall` form. The
three-step open/lookup/call form is an optimization for repeated
calls; not required for bootstrap.

**Multi-FSM scheduler**: not required if you only support single-FSM
programs initially. Single-FSM is enough for hello-world, parse-int,
small CLIs. Add multi-FSM when the host needs SDL or other event-
loop programs.

**What can be deferred entirely** (these all work in the current Rust
runtime but aren't load-bearing for "minimum effect-run"):
- Set(T) — never used in stdlib
- Real numbers — almost never used; defer until a program needs them
- Generics — defer until a program needs them
- The Z3 cache machinery (`build_cache` / `run_cached`) — start
  simple, add when profiling demands
- All of `evident query` / `evident sample` / `evident test` — these
  are convenience CLI commands, not part of `effect-run`
- Reflection (Program → Evident value) — only needed for self-hosted
  passes like the GLSL transpiler
- Async event sources — only needed for non-trivial interactive
  programs

## Bootstrapping order (suggested)

A staged path to a working `effect-run`:

1. **Lexer + parser + AST** — parse a hello-world FSM into AST. No
   solver yet; just exercise the parser.
2. **Z3 binding + minimal translator** — translate a single FSM
   declaration into Z3 constraints. Solve. Extract a model. Print
   the bindings. Don't bother with effects yet.
3. **FFI primitive** — implement `dlopen` / `dlsym` / `libffi` call.
   Test by calling libc's `getpid` from a hand-built `LibCall`
   value. Bootstrap goal: `effect-run` over a program that emits
   `LibCall(libc, getpid)` and prints the result.
4. **Effect dispatcher** — wire the dispatch loop. The hello-world
   target: a one-FSM program with `effects = ⟨LibCall("libc",
   "puts", "i(s)", ⟨ArgStr("hello")⟩), Exit(0)⟩`. Verify it prints
   and exits.
5. **Stdlib runtime.ev parsing** — get the host to load
   `stdlib/runtime.ev`, with all its enum declarations. Validates the
   parser + enum handling + datatype building.
6. **Sequential FSM with last_results** — a counter program that
   uses `last_results[0]` to thread a value across ticks.
7. **Subscription scheduling** — only relevant once you support
   multi-FSM. Add when needed.
8. **FTI bridges** — likewise.

After step 4 you have a working `effect-run` for one-shot CLI
programs. Steps 5–8 extend coverage to the rest of the use cases.

## Self-hosting the parser (the open question)

A truly minimal runtime would parse Evident with Evident — handle
only enough syntax in the host to load `stdlib/parser.ev`, which
parses the full language. This shrinks the host code substantially
(parser is the largest single component) at the cost of a more
complex bootstrap.

The pattern is well-known: Lua, Smalltalk, OCaml, and others did it.
Requires:
- A minimal hand-written parser in the host for a tiny Evident subset
- A full parser written in Evident, using primitives like string
  tokenization, list construction
- Some boot sequence: host parses the bootstrap subset → loads
  parser.ev → from then on, parser.ev parses real source

This is straightforward but big. Recommendation: defer. Build the
host runtime with a full parser, get effect-run working, port the
plugins to stdlib, then revisit the bootstrap question once the rest
is stable.

## Configuration knobs

The Rust runtime supports several environment-variable knobs.
Re-implementations should ideally support these for compatibility,
or document equivalents:

- `EVIDENT_SCHEDULER` — `legacy` falls back to the older
  "tick-every-FSM-every-iteration" scheduler; default is the
  subscription-driven path.
- `EVIDENT_TICK_MS` — frame-timer interval for `world.tick_count`
  source. Default depends on whether any FSM subscribes.
- `EVIDENT_CLOCK_MS` — WallClock plugin's poll interval.
- `EVIDENT_DISPATCH_SEED` — seed the random tie-break for
  reproducible effect-dispatch orderings.
- `EVIDENT_LENIENT` — demote translator "dropped constraint" errors
  to warnings. Default behavior is fatal.
- `EVIDENT_LOOP_TIMING` / `EVIDENT_LOOP_TRACE` — diagnostic output
  for the scheduler.

## What to read in this repo

The Rust runtime is a reference implementation. Files most useful
when implementing a new runtime:

- `runtime/src/lexer.rs` — token shapes.
- `runtime/src/parser.rs` — AST production rules.
- `runtime/src/ast.rs` — AST type definitions; the language's
  ground truth IR.
- `runtime/src/translate/` — AST → Z3. The hardest part; many
  files, each focused on one translation concern.
- `runtime/src/effect_loop.rs` — the FSM step driver.
- `runtime/src/effect_dispatch.rs` — Effect → IO mapping.
- `stdlib/runtime.ev` — the Effect/Result/FFIArg enums and event-
  source contracts. The minimum stdlib your runtime must successfully
  parse + translate.
- `examples/test_01_hello.ev` — the smallest FSM with effects.
  Run this end-to-end first.

## What this doc does not cover

- **The full language spec**: idiomatic patterns, type system
  semantics, constraint composition rules, etc. See `CLAUDE.md` for
  the canonical Evident-style guide.
- **Performance**: scaling Z3, caching strategies, multi-FSM
  parallelism. The reference runtime handles these in
  `runtime/src/translate/eval.rs` (build_cache + auto-tuner); a
  minimal runtime can defer.
- **CLI shell**: `evident query`, `test`, `sample`, etc. These are
  thin wrappers over the same `load_file` + `query` API. Add as
  needed.
- **Reflection / self-hosted passes**: a future stage where Evident
  programs walk and transform their own AST. Requires
  Program-as-Value support that the Rust runtime has but isn't
  load-bearing for `effect-run`.
