# Runtime file invariants

What each file under `runtime/src/` IS, what it depends on, and what
it must never do. These are **invariants** — properties that hold by
design, not snapshots of current state. Any drift away from them is
a violation. The code-review subagent reads this file before
reviewing any runtime source.

Note on format. Per-file briefs use plain paragraphs, four sections
each: **Purpose**, **What it must NEVER do**, **Dependencies**,
**Generic-runtime analogue**. There is no "known issues" or "TODO"
content here — those are reviewable and decay; invariants don't.

## Group 1 — Frontend (source → AST)

### `runtime/src/lexer.rs`

**Purpose.** Converts source text (a `&str`) into a flat
`Vec<Token>`. One job: tokenization. Owns the `Token` enum (the
lexical vocabulary) plus the recognizer for each token, including
the Unicode operators (`∈`, `∀`, `⇒`, `⟨⟩`, etc.), word-keyword
forms (`in`, `mapsto`, `not in`), numeric and string literals, and
significant indentation tracking via `Newline` + `Indent(usize)`
markers.

**What it must NEVER do.** Build any structure beyond a flat token
sequence — no nesting, no parse decisions, no precedence handling.
Never reference Z3, the runtime, effects, FTI, or any C library.
Never know what makes a *valid* sequence of tokens — that's the
parser's job. Errors describe character-level / token-level
malformations only (`unterminated string`, `unknown operator`),
never grammar.

**Dependencies.** Zero `use crate::*` imports. Leaf module —
depends only on `std`. Importers: `parser.rs` (consumes `Token`);
transitively, anyone parsing.

**Generic-runtime analogue.** `tokenize.c` in CPython, `Lexer.cpp`
in Clang, `lexer.go` in Go's `go/scanner`, `rustc_lexer`. Always
the bottom-most translation step; always trivial relative to the
rest of the compiler.

### `runtime/src/ast.rs`

**Purpose.** Defines the *data shape* of a parsed Evident program:
every variant of `Expr`, `BodyItem`, `MatchArm`, `MatchPattern`,
`Pins`, `BinOp`, `EnumDecl`, `Program`, etc. Pure data definitions
— no behavior, no I/O, no references to Z3 or anything else. The
shape is what every other layer agrees on.

**What it must NEVER do.** Contain library-specific data
structures (the `SdlVertex` family belongs nowhere near here).
Never depend on any other module. Never contain logic beyond
trivial derives. Never know how programs are *parsed* (parser),
*translated* (translator), *executed* (effect_loop), or
*dispatched* (effect_dispatch). The variants enumerate what's
syntactically possible; the meaning lives elsewhere.

**Dependencies.** Zero `use crate::*` imports — leaf module.
Importers: nearly every other file in the runtime.

**Generic-runtime analogue.** `ast.h` in Clang, `tree.go` in
`go/ast`, the AST types in `syn`, `Stmt` and `Expr` hierarchies in
LLVM. Universal pattern: pure data types, no methods beyond
derives, depended on by everything downstream.

### `runtime/src/parser.rs`

**Purpose.** Hand-rolled recursive-descent parser: `Vec<Token>` →
`Program`. The grammar is encoded implicitly by which Rust parser
function calls which (this is about the parser's internal Rust
function structure — Evident itself is a relational language, not
a function-based one). Handles precedence climbing for `Expr`,
indentation for `BodyItem` blocks, and the special-case parsing
for chained-membership, `match` arms, passthrough, record literals.
Diagnostics describe syntactic errors with `(line, col)`.

**What it must NEVER do.** Do semantic checking — names not bound,
types not matching, claims not found. That's the translator's job.
Never reach into Z3. Never call any C library. Never directly emit
translated output; the result is always `Program`. Should not hold
mutable global state. Should not depend on `pretty.rs`.

**Dependencies.** `crate::ast::*` (builds AST nodes) +
`crate::lexer::Token` (consumes tokens). Two upstream modules, no
downstream dependencies. Importers: `runtime.rs` (top-level
`load_source`).

**Generic-runtime analogue.** `parser.go` in Go, `Parser.cpp` in
Clang, hand-rolled parsers in most language implementations
(LLVM, Roslyn). Recursive-descent over PEG / packrat / GLR is the
chosen tradeoff: hand-tuned error messages beat generated parsers
for a small grammar.

### `runtime/src/pretty.rs`

**Purpose.** AST → readable infix string. Used purely for
diagnostics — when Z3 returns UNSAT, we want to show the user
their original-feeling syntax, not the canonical fully-parenthesized
form.

**What it must NEVER do.** Round-trip — this is *not* the inverse
of the parser. Lossy by design (doesn't preserve original
parenthesization). Never depend on Z3 (it's pre-translation).
Never depend on the runtime (it's pure-AST). Never grow into a
serializer (no JSON output, no spec emission); a serializer is a
separate module if we want one.

**Dependencies.** `crate::ast::{BinOp, BodyItem, Expr, Mapping,
MatchPattern}`. Pure read of AST data. Importers: any module
producing user-facing diagnostics.

**Generic-runtime analogue.** `Stmt::dump()` in Clang,
`fmt::Display for Expr` in `syn`, the AST printer in TypeScript's
compiler. Always exists; always small relative to the parser;
always lossy.

## Group 2 — Translation (AST → Z3)

The translate pipeline turns a parsed `Program` into Z3 constraints,
runs the solver, and reads model values back out. Internal layering:
`types.rs` is the data leaf; `datatypes.rs`, `declare.rs`, and
`preprocess.rs` build on `types`; `exprs.rs` builds on those;
`inline.rs` orchestrates expression + declaration over a claim
body; `eval.rs` orchestrates everything for a query. `encode_ast.rs`
and `decode_ast.rs` are a parallel AST-roundtrip pair for
self-hosted compiler passes.

### `runtime/src/translate.rs`

**Purpose.** Module entry. Declares the `translate/` sub-modules
and re-exports the small set of public items (`evaluate`,
`build_cache`, `run_cached`, `sample_cached_inner`, `Value`,
`EvalResult`, `FieldKind`, `DatatypeRegistry`, `CachedSchema`,
`structural_names`, `structural_signature`). The `pub use`
re-exports define the translate module's external API.

**What it must NEVER do.** Contain implementation logic. Stays a
small file whose body is `mod x;` and `pub use`. Never widen the
re-export list to expose translate-internal types — the boundary
exists on purpose. Never depend on anything outside the runtime
crate.

**Dependencies.** None. Purely a module-organization file.

**Generic-runtime analogue.** The `mod.rs` of any Rust subsystem
that has internal layering (e.g. `rustc_codegen_ssa::back::mod.rs`,
`rustc_resolve::late::mod.rs`).

### `runtime/src/translate/types.rs`

**Purpose.** Defines the typed bindings shared by every other file
in the translate pipeline: `Var` (a typed Z3 const plus metadata
about what kind of variable it is), `Value` (an extracted model
output, the user-facing shape of a query result), `FieldKind`
(composite-field metadata for record / Seq-element access),
`EnumRegistry`, `DatatypeRegistry`, `CachedSchema`, `EvalResult`.

**What it must NEVER do.** Contain translation logic. No Z3
expression construction, no Solver use, no constraint assertion.
The boundary: data and trivial constructors only; behavior
belongs in the consumers. Never know about Effects, the
scheduler, or FFI — these types describe the constraint side of
the runtime, not the execution side.

**Dependencies.** Imports `z3` directly (its types wrap Z3
values) and `ast` (some `Var` variants reference AST node
references). No `use super::*` — leaf within translate/.

**Generic-runtime analogue.** The typed-IR data structures of any
compiler — Clang's `QualType` family, LLVM's `Type` taxonomy,
GHC's `TyCon` and friends. Pattern: pure typed data, depended on
by all subsequent passes.

### `runtime/src/translate/datatypes.rs`

**Purpose.** Builds Z3 `DatatypeSort`s for user-defined types
that appear as the element type of a `Seq(UserType)`. Caches
results in the shared `DatatypeRegistry` so two users of the same
nested type (e.g. `SDLRect.color` and `SDLOutput.bg` both pointing
at `Color`) share one Z3 sort.

**What it must NEVER do.** Build Z3 *expressions* — only sorts.
Never assert constraints, never call the Solver. Never own the
`DatatypeRegistry` it writes to — it borrows.

**Dependencies.** `types` (for `DatatypeRegistry`, `FieldKind`),
`ast`.

**Generic-runtime analogue.** Type-table / sort-table builders in
any SMT-backed system; type-environment construction in Hindley-
Milner inference.

### `runtime/src/translate/declare.rs`

**Purpose.** Turns a `Membership` AST item into a typed Z3
constant in an environment, recursing into sub-schemas to expand
their fields. Owns `CLAIM_CALL_COUNTER` (and `next_call_id`) used
to generate per-invocation suffixes when a claim is inlined more
than once in the same query (so the second call's internal
variables don't collide with the first's).

**What it must NEVER do.** Never call into `eval` or `extract`.
Never know what an Effect is. (The "must not assert constraints"
half is now mechanically enforced by AP-009.)

**Dependencies.** `types`, `datatypes`, `ast`.

**Generic-runtime analogue.** "Symbol table population" / "name
binding" pass — the part of a compiler that walks declarations
and registers bindings without yet acting on them.

### `runtime/src/translate/preprocess.rs`

**Purpose.** Pre-translation passes that operate on the AST
before any Z3 work happens: pin literal-int variables, propagate
Seq lengths, fold quantifier bounds. The point is to surface
concrete integers where possible so the downstream translator can
unroll quantifiers, fold cardinalities, and produce smaller Z3
formulas.

**What it must NEVER do.** Assert constraints. Use a Solver. The
pass shape is pure: input AST + small `Value` map → refined AST +
updated `Value` map. (The "must not build Z3 expressions" half is
now mechanically enforced by AP-010 — Z3 expression construction
belongs in `exprs.rs`.)

**Dependencies.** `types`, `ast`. Notably NOT `exprs` — the
literal-folding helper that used to live in `exprs` and was
imported here was promoted down to `types` to break the cycle
(see AP-011).

**Generic-runtime analogue.** Constant-folding / partial-
evaluation / dead-code-elimination passes in classical compilers.

### `runtime/src/translate/exprs.rs`

**Purpose.** AST `Expr` → Z3 expression translators, one per Z3
sort: `translate_int`, `translate_bool`, `translate_str`,
`translate_real`. Plus the helpers they share: `resolve_mapping`
and `expr_as_var` for `ClaimCall` mapping resolution;
`translate_seq_lit_eq` and `translate_seq_index_assign` for the
two seq-equality shapes that aren't pure scalar `_eq`.

**What it must NEVER do.** Declare new Z3 constants — that's
`declare`. Assert constraints — that's `inline`. Call into
`eval`. The translation is pure: `(env, ast) → z3-expr`.

**Dependencies.** `types`, `ast`. Helpers shared between
`preprocess` and `exprs` (env utilities, literal-range queries)
belong in `types` so both can borrow without forming a loop.
(The "no mutual import between preprocess and exprs" half is now
mechanically enforced by AP-011.)

**Generic-runtime analogue.** "Expression codegen" / "expression
elaboration" — the part of a compiler that emits IR for one
expression at a time given an environment.

### `runtime/src/translate/inline.rs`

**Purpose.** The recursive walker over a claim's `BodyItem`s.
Per item: `Membership` → declare via `declare`; `Constraint` →
translate via `exprs` and assert on the Solver; `Passthrough`
(`..ClaimName`) → recurse into the named claim's body;
`ClaimCall` → resolve mappings, generate per-invocation fresh
names for unmapped internals, recurse. The orchestration layer
between expression translation and constraint assertion.

**What it must NEVER do.** Own the Solver (borrows it). Own the
registries (borrows them). Decide what's a "schema" vs "claim"
vs "type" — that's a load-time keyword distinction the parser
already made. Know about Effects, the scheduler, or any I/O.

**Dependencies.** `types`, `declare`, `exprs`, `ast`, `pretty`
(for human-readable diagnostics on constraints that fail to
translate).

**Generic-runtime analogue.** "Statement-level codegen walker" —
the recursive loop that emits IR for a function's body in a
classical compiler.

### `runtime/src/translate/extract.rs`

**Purpose.** Reads model values back out of a satisfied Solver.
One function per `Var` kind (Int, Bool, Real, Str, Handle, Enum,
record, Seq…) mapping the Z3 binding to a `Value`. Also owns
`assert_seq_given`, the inverse direction: pinning a Seq variable
to a `Value::Seq*` shape from a caller-supplied `given` map.

**What it must NEVER do.** Build new constraints. Declare new
vars. Recurse into claim bodies — extraction is leaf-level
(applies per Var, not per claim).

**Dependencies.** `types`. No `ast` import — operates on
`Var`/`Value`, not raw AST.

**Generic-runtime analogue.** "Model interpretation" in SMT-
backed tools — the part that reads the SAT solver's model and
converts it to user-facing values.

### `runtime/src/translate/eval.rs`

**Purpose.** The public orchestrator entry points: `evaluate`
(one-shot query), `build_cache` + `run_cached` (per-step cached
query for the multi-FSM scheduler), `sample_cached_inner`
(n-distinct-model sampling), plus `evaluate_with_extra_assertion`
/ `_core` variants for unsat-core extraction. Wires together
declare + inline + extract + Solver + the arithmetic-tuner that
picks `smt.arith.solver` per query shape.

**What it must NEVER do.** Define or modify the typed-binding
model (that's `types`). Own the AST shape — only consume it.
Know what an Effect is — it produces solver results, not
side-effects. Own a CLI command's UX (that's `commands/`).
Scatter `use crate::*` / `use super::*` imports through the file
body — all crate-internal imports go at the top of the file
where any reader can see the dependency surface at a glance.

The file has four distinct sub-concerns that must stay cleanly
sectioned (with `// ──` headers between them) and ordered so
each section depends only on those above it: (1) numeric and
solver-tuning helpers; (2) the cached-query path (`build_cache`,
`run_cached`, `sample_cached_inner`) used by the multi-FSM
scheduler; (3) the one-shot evaluate variants used by `query` /
`check` / `sample` CLI commands; (4) local model-extraction
helpers used by both query paths. Mixing sub-concerns within a
section is a violation. If a single section grows large enough
that it needs its own internal helpers + multiple public
entries, that's the signal to split it into its own file under
`translate/eval/`.

**Dependencies.** Most of `translate/`'s siblings (types,
declare, inline, exprs, extract, preprocess), `ast`.

**Generic-runtime analogue.** The top-level "compile" / "solve"
/ "interpret" entry points in any system — the thin orchestration
layer that ties an internal pipeline together.

### `runtime/src/translate/encode_ast.rs`

**Purpose.** Encodes a parsed `Program` (Rust AST) as a Z3
`Datatype` value matching the shape declared in `stdlib/ast.ev`.
The bridge that lets self-hosted compiler passes consume real
source: pass writers consume a `Program` as a `given`; this
module produces the `given` value from parsed input.

**What it must NEVER do.** Be the AST source of truth — `ast.rs`
is. Build constraints. Run a Solver. The conversion is pure:
Rust AST → Z3 Datatype value.

**Dependencies.** `types` (for `EnumRegistry`), `ast`. Also
implicitly coupled to `stdlib/ast.ev` — the Z3 Datatype shape
this file produces must structurally match the enum declarations
in `stdlib/ast.ev`. Any change to `stdlib/ast.ev` requires a
matching change here. The coupling isn't visible from imports;
it's enforced by runtime failure (encoding produces a malformed
Datatype value if shapes drift).

**Generic-runtime analogue.** "Deparse" / "AST serializer" used
to feed an AST through a self-hosted compiler pass. In Lisp
terms: the printer side of the reader/printer roundtrip.

### `runtime/src/translate/decode_ast.rs`

**Purpose.** Inverse of `encode_ast`. Z3 model `Value`
(specifically `Value::Enum` trees matching `stdlib/ast.ev`) →
Rust `ast::Program`. Used after a self-hosted desugar pass
produces a transformed `Program` in the model — this module
reads it back so the runtime can replace the loaded `Program`
with the transformed one.

**What it must NEVER do.** Invent AST nodes from scratch — it
*reconstructs*, never *creates*. Know about Effects, FFI, or the
scheduler beyond the variants the AST defines. Accept Values
that aren't Enum-shape — fail fast on shape mismatch.

**Dependencies.** `types` (for `Value`), `ast`. Also implicitly
coupled to `stdlib/ast.ev` — the Z3 Datatype shape this file
consumes must structurally match the enum declarations in
`stdlib/ast.ev`. Any change to `stdlib/ast.ev` requires a
matching change here AND in `encode_ast.rs`; the two must stay
in lockstep.

**Generic-runtime analogue.** Deserializer in any AST-
roundtripping system; reader in Lisp.

## Group 3 — Runtime API + analysis

The `runtime.rs` facade is what every external caller (commands,
tests, embedders) goes through. `subscriptions.rs` is a single
static-analysis pass that produces derived AST data needed by the
multi-FSM scheduler.

### `runtime/src/runtime.rs`

**Purpose.** The crate's top-level public API. Owns
`EvidentRuntime` (the per-process facade), the global Z3
`Context`, the schema/enum registries the runtime has loaded, and
the per-claim cached `CachedSchema`s for repeated queries. Exposes
the verbs callers actually use: load source / file, query a
claim by name (with or without `given` bindings, with or without
unsat-core extraction), inspect what's loaded, encode an AST as
a Z3 value for self-hosted passes, replace a body item in a
loaded claim. It's the only file in the crate that holds
long-lived state.

**What it must NEVER do.** Re-implement parsing (delegates to
`parser`). Re-implement translation or solving (delegates to
`translate`). Build Z3 expressions directly. Own a CLI
subcommand or any UX — `commands/` files call into this facade,
not the other way around. Know about Effects, the multi-FSM
scheduler, FFI, FTI, library bridges, or anything in the
"execution" or "foreign" layers. The runtime API is for
constraint solving over loaded programs; the execution layer
sits on top.

The wide method surface (load + query + introspection +
self-hosted-pass support) is the cost of being THE facade. New
verbs go here only if they fit the same shape (operate on
loaded program state, return a result or modify the registry);
anything that doesn't fit that shape lives in its own module.

**Dependencies.** `ast`, `parser`, `translate` (public
re-exports only — no `translate::eval::*` reaches in past the
boundary), `z3`. No use of any other internal module.

**Generic-runtime analogue.** The top-level `Compiler` /
`Engine` / `Interpreter` facade in language implementations —
`rustc::Session`, `clang::CompilerInstance`, V8's `Isolate`.
Always the public mouth of the implementation; always small in
concept (a state container + verb methods) even when wide in
surface.

### `runtime/src/subscriptions.rs`

**Purpose.** A single static-analysis pass over a `SchemaDecl`'s
body. Produces an `AccessSets { reads: Set<String>, writes:
Set<String> }` describing which `world.X` fields the claim
reads and which `world_next.X` fields it writes. Used by the
multi-FSM scheduler to decide which FSMs need to wake when a
particular world field changes. Also exposes
`body_references_identifier` for ad-hoc identifier searches.

**What it must NEVER do.** Touch the Solver. Touch any
translation state (no `EnumRegistry`, no `DatatypeRegistry`, no
`Var` bindings). Look at runtime state — it's a pure AST → Set
function. Cause side effects. Resolve `..ClaimName` passthroughs
or `ClaimCall` invocations recursively — the current
implementation treats them as opaque, and any caller that needs
the transitively-resolved set must walk it themselves. Know
about Effects, FTI, scheduling state, or any C library.

**Dependencies.** `ast` only. Pure leaf with respect to the rest
of the crate.

**Generic-runtime analogue.** Use-def / free-variable analysis
passes in classical compilers — pure AST walks producing a
derived set. Examples: GHC's free-variable analysis, the simpler
forms of liveness analysis, the "captures" detection in
closure-converting compilers.

## Group 4 — Execution

The execution layer turns "loaded program with `effects` and
`last_results` shape" into an actual running multi-FSM program.
`effect_loop.rs` is the scheduler; `effect_dispatch.rs` is the
per-effect performer.

### `runtime/src/effect_loop.rs`

**Purpose.** The multi-FSM scheduler. At startup, walks every
loaded claim and detects which have the FSM shape (`state,
state_next ∈ <enum>` + `last_results ∈ ResultList` + `effects ∈
EffectList`); installs any FTI bridges those claims declare.
Per tick: decides which FSMs to wake (subscription-driven by
world-field reads/writes, state self-feedback, effect
self-feedback, external event sources); for each woken FSM,
solves the claim via the runtime's cached-query path with
state and last_results pinned; decodes `state_next` and
`effects` from the model; dispatches the effects via
`effect_dispatch`; propagates `world_next.*` writes into the
world snapshot. Halts when no FSM is scheduled in a tick or any
FSM emits `Effect::Exit(code)`.

**What it must NEVER do.** Build Z3 expressions or run the
Solver directly — solving goes through the runtime facade.
Decode model values directly — uses the AST decoder. Open C
libraries or call libffi — that's the FFI layer. Perform an
Effect itself — always delegates to `effect_dispatch`. Carry
per-tick state in module-level globals — all state lives in
`LoopResult` / per-FSM context structs threaded through the
call graph.

The scheduler's concerns are: which FSMs to wake (subscriptions,
self-feedback, external events), when to halt, how to thread
state and effects per tick. Anything outside that — what kinds
of background event sources exist, how typed C resources get
installed at startup — is NOT a scheduler concern. The scheduler
should run correctly against any collection of objects that can
wake FSMs, without knowing how that collection was assembled or
what each object's specific origin is. Adding a new typed C
resource (SDL_Audio, etc.) or removing the FTI mechanism entirely
should not require touching this file. (The "no `use` of any
specific bridge struct type" half of this invariant is now
mechanically enforced by AP-012.)

**Dependencies.** `ast` (BodyItem + EffectResult shape),
`effect_dispatch` (DispatchContext + dispatch_all), `runtime`
(EvidentRuntime facade for solving), `translate` (Value +
ast_decoder for reading models), and an abstraction over event
sources sufficient to receive wake events and read source-
written world fields.

**Cross-file contracts.** The scheduler reads
`DispatchContext::pending_spawns` after each `dispatch_all`
returns; this is the channel by which `Effect::SpawnFsm`
delivers a new FSM into the scheduler. Both files must change
together when that protocol changes. See the matching note in
the `effect_dispatch.rs` invariant.

**Generic-runtime analogue.** A reactive event-loop scheduler.
Closest analogues: the BEAM scheduler in Erlang/Elixir (every
process is a small state machine; the scheduler picks ready
processes per tick), game-engine main loops with multiple
subsystems, or an actor-model runtime's dispatcher. The fact
that we use a constraint solver to compute next-state instead
of direct mutation is the unusual part; the scheduling shape is
classical.

### `runtime/src/effect_dispatch.rs`

**Purpose.** Turns an `Effect` value into an `EffectResult` by
actually performing the side effect. Owns `DispatchContext` —
the per-program-run mutable state: `lib_cache` (path → loaded
library handle), `sym_cache` ((lib, symbol) → resolved
function-pointer handle), `exit_requested` (set by graceful
`Effect::Exit`), `pending_spawns` (filled by
`Effect::SpawnFsm`), the FFI handle registry, and the input /
output streams (so a test can swap stdin/stdout for capture).
Owns `DispatchMode` (`Real` for actual execution, `Replay` for
trace-test playback against `RecordedCall`s). Per Effect: built-
ins (Print, Println, ReadLine, Time, Exit, ParseInt, ParseReal,
IntToStr, RealToStr, ShellRun, SpawnFsm) hit the OS / runtime
directly; FFI primitives (FFIOpen, FFILookup, FFICall, LibCall,
CloseHandle) route through `ffi.rs`; `Effect::Seq` is unwrapped
by `dispatch_seq` which also resolves `ArgPriorResult(N)` to the
Nth prior call's typed result.

**What it must NEVER do.** Build Z3 expressions, run the
Solver, or do anything constraint-related. Schedule FSMs —
that's `effect_loop`'s job. Decide which Effects exist (those
are AST variants in `ast.rs`); only know how to dispatch the
ones it sees. Contain library-specific code beyond the generic
FFI primitives — every C library it ever calls comes through
`Effect::LibCall` or `Effect::FFICall` with caller-supplied
path + symbol + signature. Hold any global mutable state —
`DispatchContext` is per-call, threaded through the dispatch
functions.

**Dependencies.** `ast` (Effect + EffectFfiArg + EffectResult),
`ffi` (the libffi marshaling layer). Notably NOT `event_sources`
or `fti` — those are scheduler-side concerns. Notably NOT
`translate` or `runtime` — dispatch knows nothing about
constraints.

**Cross-file contracts.** `Effect::SpawnFsm` is dispatched by
queueing onto `DispatchContext::pending_spawns`; dispatch never
instantiates an FSM itself. The scheduler (`effect_loop`) is
responsible for draining `pending_spawns` after each
`dispatch_all` returns and acting on each request. Both files
must change together when the spawn protocol's shape (the field
on `DispatchContext`, the request payload type) changes. See
the matching note in the `effect_loop.rs` invariant.

**Generic-runtime analogue.** A syscall dispatcher / effect
handler. Closest analogues: Haskell's IO action interpreter
(the part that turns `IO a` into actual side effects), an OS
syscall trap handler, a game engine's command processor, the
"stage" in a stage-and-perform monadic effect system. Always
sits at the boundary between pure / specified and impure /
performed.

## Group 5 — FFI / FTI / Bridges

The boundary at which library-specific knowledge enters the
runtime. `ffi.rs` is generic C-ABI marshaling (knows nothing
about any particular library); `fti.rs` is a registry mapping
Evident type names to bridge install functions; `event_sources/`
holds one file per typed C-resource bridge (SDL_Window,
GL_Program, FrameTimer, etc.) and is the only place where
library-specific Rust code may live.

**Definition: bridge.** A *bridge* is a Rust struct that owns
the lifecycle of one C-side resource — a window, a GL program,
a periodic timer, a signal handler, a file reader, an external
process — and implements the `EventSource` trait so the
scheduler can drain its writes and receive its wake events.
Each bridge is constructed via an install function registered
in `fti.rs::INSTALLERS`. The user references it from Evident
through a typed declaration like `win ∈ SDL_Window (title ↦
"X", …)` — the Evident type name resolves through the registry
to the bridge that implements the resource. "Bridge" because it
sits between the Evident model and the C-side reality. When a
bridge needs another bridge's output (e.g., `GL_Program` needs
`SDL_Window`'s GL context), the dependency is expressed in
Evident at the user's declaration site, not in Rust imports
between bridge files.

### `runtime/src/ffi.rs`

**Purpose.** The libffi calling-convention bridge. Wraps `dlopen`
(via `libloading::Library::new`), `dlsym` (via `Library::get`),
and the libffi `Cif::call` machinery behind a small enum surface:
`FfiArg` (an Evident-typed argument), `FfiReturn` (a typed
result), `FfiError`. Owns `HandleRegistry`, a generic typed-
pointer registry that hands out `u64` IDs for libraries,
function pointers, and any other opaque pointer the FFI layer
manages. Owns the type-code parser (`i` → int64, `s` → const
char*, etc.) and the per-arg marshaling that packs Evident
values into the slots libffi expects.

**What it must NEVER do.** Contain library-specific knowledge.
No `SDL_`, no `gl[A-Z]`, no hardcoded dylib paths beyond what
callers supply as arguments to `ffi_open(path)`. No special-case
arg variants for one library's struct layout (the `SdlVertexBuf`
intrusion in this file is the canonical AP-001 violation).
Build Z3 expressions or run the Solver. Schedule FSMs. Know
about Effects — it's pure machinery; callers (`effect_dispatch`)
translate Effect arguments into FfiArgs.

**Dependencies.** `libffi`, `libloading`, `std`. Notably ZERO
crate-internal imports — pure leaf within the runtime. Importers:
`effect_dispatch` (the only caller), and any test that exercises
the FFI primitive directly.

**Generic-runtime analogue.** Any language's FFI bridge layer —
Python's `ctypes`, Lua's `luajit ffi`, Node's `ffi-napi`,
Ruby's `Fiddle`. Always generic over C ABIs and never about
specific libraries.

### `runtime/src/fti.rs`

**Purpose.** A single dispatch table (`INSTALLERS: &[(name,
install_fn)]`) mapping Evident type names — declared in
`stdlib/runtime.ev` as `type SDL_Window`, `type FrameClock`,
etc. — to install functions that construct and start the
matching C-resource bridge. The boundary where "user code
declares `win ∈ SDL_Window`" connects to "Rust code installs an
`SdlWindowSource`." Exposes `is_fti_type(name)` for the FSM
detector to recognize an FTI parameter, and `fti_install_fn(name)`
for the scheduler to dispatch the install.

**What it must NEVER do.** Contain bridge logic — only the
table and the install dispatcher's plumbing. The table's entries
reference install functions that live in `event_sources/<name>.rs`;
fti.rs imports those by name but does not implement them.
Build constraints, schedule, perform Effects. Hold any state —
the registry is a static `&[...]`.

**Dependencies.** `ast` (for `Pins`, the user's `(field ↦
value)` data passed as install config), `event_sources` (the
bridge struct types referenced by install fns). Notably NOT
`runtime`, `effect_loop`, `effect_dispatch`, `translate`, or
`ffi` directly — fti is the registry, nothing more.

**Cross-language contract.** Every name in `INSTALLERS` must
correspond to a `type X` declaration in `stdlib/runtime.ev` with
fields matching what the install function pins. Adding an FTI
type means: add the `type` to `stdlib/runtime.ev`, add the
install fn to a new `event_sources/<name>.rs`, add the row to
`INSTALLERS` here. Any other change set indicates the addition
isn't following the contract.

**Generic-runtime analogue.** Any plugin / capability registry's
dispatch table — MIME-type → handler maps in browsers, the
syscall table in a kernel, capability registries in Android,
the registry of mounted filesystems in a Unix kernel. Pattern:
a small static table; entries don't know about each other; the
table is the only thing that has to grow when the system grows.

### `runtime/src/event_sources/<name>.rs` (one file per bridge)

**Purpose.** Each file owns the lifecycle of one typed C
resource — a window, a GL program, a periodic timer, a signal
handler, a file reader, an external process. Declares one `pub
struct <Name>Source`, implements `EventSource` (the trait that
defines wake + write-queue semantics), and provides a constructor
that takes the install-time configuration. The struct holds
whatever state the resource needs across its lifetime (handles,
background-thread join handles, channels). This layer is the
only place in the runtime where library-specific Rust code may
live.

**What it must NEVER do.** Reach into `runtime`, `effect_loop`,
`effect_dispatch`, or `translate`. Communicates with the
scheduler only through the `EventSource` trait surface (wake
channel + `drain_writes()`) — never by importing scheduler
types or reaching up. Build Z3 expressions or know that Z3
exists. Cross between bridge files: each
`event_sources/<name>.rs` knows about its own resource and
nothing about any sibling bridge. If two bridges need shared
helpers, those helpers live in `event_sources/mod.rs` (the
trait + queue + channel definitions) or in their own utility
file under `event_sources/`.

**What each file MUST contain.** Exactly one `pub struct
<Name>Source`. Its `impl EventSource for <Name>Source`. A
constructor (`pub fn new(...)` or `<Name>Source::start_inline`
for synchronous installs). A `Drop` impl when the bridge owns
threads or resources that need explicit teardown.

**Dependencies.** `event_sources::mod` (the trait + shared
channel/queue types), the C library this bridge wraps (via
`libloading` or direct `extern "C"` declarations), `std`.
Bridges may NOT import each other; if a bridge needs another
bridge's resource (e.g., GL_Program needs SDL_Window's GL
context), the user's Evident program declares both and the
scheduler arranges install order — bridges don't reach across.

**Generic-runtime analogue.** Per-driver / per-device file in a
kernel — `drivers/usb/usb_storage.c`, `drivers/gpu/i915.c`. Each
file owns one piece of hardware (or one C library); the
overall driver framework / scheduler doesn't know what's inside.
Adding hardware = adding a file, not modifying existing ones.

### `runtime/src/event_sources/mod.rs`

**Purpose.** Owns the `EventSource` trait (the contract every
bridge implements), the `SchedulerEvent` enum (what bridges
push onto the wake channel), the `WriteQueue` type (what
bridges deposit world-field writes onto), and the helpers
(`new_write_queue`, `drain`) that adapt between bridges and
the scheduler. The shared abstraction layer all bridges build
on.

**What it must NEVER do.** Implement any specific bridge. Know
about specific C libraries. Carry any per-bridge state. The
trait + supporting types + helpers — nothing more.

**Dependencies.** `std` only. Notably zero crate-internal
imports beyond `crate::Value` (the type-of-write payload, which
is part of the runtime's Value model). Generic over what
bridges do.

**Generic-runtime analogue.** The base trait/interface layer in
any plugin system — `Driver` in a kernel's driver framework,
`Plugin` in a host-plugin system, `Filesystem` in an OS's VFS
layer. Always small, always pure data + traits, depended on by
every concrete plugin.

## Group 6 — CLI (commands/)

The `commands/` directory holds one file per `evident <subcommand>`
verb plus a shared-helpers file. These files are the only things in
the runtime that talk to `argv` / `stdout` / `stderr` / `ExitCode`;
everything below them is a pure library. The commands consume the
runtime via the published facade (`evident_runtime::EvidentRuntime`,
`evident_runtime::Value`, etc.) and never reach into internal
modules.

### `runtime/src/commands.rs`

**Purpose.** Module entry. Declares the `commands/` sub-modules (one
per CLI subcommand) plus `common`. Body is just `pub mod x;`
declarations.

**What it must NEVER do.** Contain implementation. Re-export
internals beyond what `pub mod` already does. Carry state.

**Dependencies.** None.

**Generic-runtime analogue.** The `mod.rs` of any
subcommand-organized CLI tool (`cargo`'s `bin/cargo/commands/mod.rs`,
`git`'s `builtin/` listing, `rustc_driver`'s subcommand glue).

### `runtime/src/commands/common.rs`

**Purpose.** Shared helpers used by multiple `cmd_*` files: usage
banner, generic argv splitting, flag parsing (`--given`, `--json`,
`-n`, etc.), runtime construction (`load_runtime` reads the file
list and returns a loaded `EvidentRuntime`), value formatting
(text + JSON), the SAT/UNSAT printer used by both `query` and
`sample`. Type-inference helpers like `infer_value` for parsing
`--given k=v` strings into typed `Value`s.

**What it must NEVER do.** Belong to one specific command — if
helpers are only used by a single `cmd_*` file, they live in
that file, not here. Reach into runtime internals — uses only
`evident_runtime::*` (the public facade). Hold state across
calls.

**Dependencies.** `evident_runtime` (the public facade), `std`,
`std::process::ExitCode`. Importers: every `cmd_*.rs`.

**Generic-runtime analogue.** The shared "argparse helpers + I/O
formatters" file every multi-command CLI grows — `git`'s
`parse-options.c`, the helpers in `cargo/src/bin/cargo/cli.rs`,
the common table in `pip`.

### `runtime/src/commands/check.rs`, `query.rs`, `sample.rs`, `effect_run.rs`, `lint.rs`

**Purpose (each).** One CLI subcommand verb. Each file declares
exactly one `pub fn cmd_<name>(args: &[String]) -> ExitCode` —
the entry point invoked from `main.rs`'s subcommand dispatch.
Each follows the same skeleton: parse args (via `common::Flags`
where shared, custom otherwise), construct an `EvidentRuntime`
(via `common::load_runtime`), call into the public runtime API
to do the work, format the result for stdout/stderr, return an
ExitCode.

**What each MUST NOT do.** Build Z3 expressions, run the Solver
directly, decode model values manually — all that goes through
the runtime API. Reach into `crate::*` runtime internals — uses
only `evident_runtime::*` re-exports. Schedule FSMs (that's
`effect_run`'s thin glue around `effect_loop::run`, but
`effect_run` doesn't reimplement the loop). Print except via the
formatters in `common.rs` where shared output is involved
(per-command custom output is fine in the per-command file).
Carry state across calls — each `cmd_*` is invoked once per
process invocation.

**Per-command size cap (soft).** Each subcommand should fit in
~100 lines. If it grows past that, the verb has accreted
multiple concerns and should either split into helper functions
in the same file (organized by concern) or — if a concern is
shared with other commands — promote helpers into `common.rs`.

**Dependencies (each).** `evident_runtime::*` (the public
facade), `super::common::*` (the local shared helpers), `std`,
`std::process::ExitCode`.

**Generic-runtime analogue.** Per-subcommand files in any
multi-command CLI: `cargo/src/bin/cargo/commands/build.rs`,
`git`'s `builtin/checkout.c`, `kubectl`'s `pkg/cmd/get/get.go`.
Pattern: thin entry, shared helpers, no logic that wouldn't be
useful from a non-CLI caller.

### `runtime/src/commands/test.rs`

**Purpose.** The `evident test [path]` runner. Walks a directory
for `test_*.ev` files, loads each, enumerates claims whose name
starts with `sat_` or `unsat_`, queries each, prints per-test
pass/fail/error with optional unsat-core diagnostics. Owns the
human-readable + machine-readable output formats (default
human; `--no-color` / `NO_COLOR` for plain text). Larger than
the other `cmd_*` files because the "discover + run + report
+ format" loop has real work to do, but follows the same
no-state-across-calls / no-internal-reach rules.

**What it must NEVER do.** Build constraints itself. Decode
models manually — the per-test query result comes back as a
`QueryResult` from the public facade. Skip / xfail tests
silently — the runner reports every test it discovered, with
its real outcome, and the user (not the runner) decides what
to do about failures. Carry state across runs; each invocation
is fresh.

**Dependencies.** `evident_runtime::*` (the public facade),
`super::common::*`, `std`. Notably uses `evident_runtime::pretty`
for diagnostic AST printing on unsat-core display, and
`evident_runtime::translate::preprocess_api` for collecting
referenced names (used in the "what variables does this test
constrain" diagnostic).

**Generic-runtime analogue.** Test-runner subcommands in CLI
toolchains: `cargo test`'s output formatter, `pytest`'s console
plugin, `go test`'s standard output. The pattern is universal:
discover → run → report. Color, JSON output, and timing
formatting are the variations.

### `runtime/src/commands/infer_types.rs` and `desugar.rs`

**Purpose.** Two CLI subcommands that double as libraries used
by other parts of the CLI. `infer_types.rs` runs the
self-hosted type-inference passes (`stdlib/passes/{literal_types,
iter_types,propagation,consistency}.ev`) over user source and
either prints inferences (`evident infer-types`) or applies
them automatically (called as a library from `cmd_query` /
`cmd_check` / `cmd_test`). `desugar.rs` runs the self-hosted
desugar pipeline (`stdlib/passes/desugar_passthrough.ev`) and
either reports rewrites or applies them.

The dual role is the file's defining property: each exposes
both a `pub fn cmd_<name>` for the user-facing subcommand AND
a `pub fn auto_apply_*` (plus supporting types like `Inference`
or `Rewrite`) for use as a library from sibling `cmd_*` files.

**What they must NEVER do.** Build the inference / desugar logic
in Rust — every actual rule lives in `stdlib/passes/*.ev` as
self-hosted Evident passes. The Rust files only orchestrate:
load the pass file, run a query against the user's source, decode
the resulting `Program` value back to Rust AST, apply.
Special-case any specific rule — if a rule needs special
handling, that's a sign the rule should be expressed differently
in its `.ev` file.

**Cross-language contract.** These files are coupled to the
specific pass `.ev` files they load. The pass files' structure
(claim names, expected query shape, output Datatype shape) is
part of the contract — if either side changes, both must change.

**Dependencies.** `evident_runtime::{EvidentRuntime, Value}`,
`evident_runtime::ast::*` (for AST types they receive back from
decoded passes). Importers: each other (`desugar` is called by
`infer_types`'s pipeline, or vice versa, depending on
ordering), and the `cmd_*` siblings that auto-apply.

**Generic-runtime analogue.** Self-hosted compiler passes
exposed both as standalone tools and as library hooks —
`gofmt` / `go fmt`, `rustfmt` / `cargo fmt`, the way `clippy`
is both a binary and a Cargo subcommand library. The pattern is
"a pass that the toolchain can run on its own AND that other
toolchain stages can invoke."

## Group 7 — Top-level

The two files at the root of `runtime/src/`. `lib.rs` is the
crate's library entry — it defines what the crate publishes to
the world. `main.rs` is the binary entry — it's the `fn main()`
that the `evident` executable starts at.

### `runtime/src/lib.rs`

**Purpose.** Declares the public API surface of the
`evident_runtime` library crate. Lists which sub-modules are
visible to external callers (commands/, tests/, embedders) and
re-exports the top-level facade types (`EvidentRuntime`,
`QueryResult`, `Value`) so callers can write
`use evident_runtime::EvidentRuntime` without knowing they live
under `runtime::`.

**What it must NEVER do.** Contain implementation. Re-export
everything indiscriminately — the public API is intentionally
narrow. A sub-module is `pub mod` only if external callers
need to reach into it; otherwise it's `mod` (crate-internal).
A type is `pub use`'d at the crate root only if it's part of
the canonical facade — niche internal types remain accessible
only through their owning module's path. Hold state. Wire
together components — that's the runtime facade's job, not the
crate-entry file's.

**Dependencies.** None. It's the top of the dependency graph.

**Generic-runtime analogue.** The `lib.rs` of any Rust library
crate; the `__init__.py` of a Python package's top-level; the
`Index.cmake`/`include` master header of a C++ project. Its
size and complexity should stay near the bottom of the codebase
even as the rest grows — a wide `lib.rs` is a sign the public
API has accumulated rather than been designed.

### `runtime/src/main.rs`

**Purpose.** The `evident` binary's entry point. Reads `argv`,
dispatches the first argument to the matching `cmd_<name>`
function under `commands/`, returns the resulting `ExitCode`.
Handles the no-arg / `--help` / unknown-subcommand cases with
a usage banner. Nothing else.

**What it must NEVER do.** Contain command logic — every verb's
work lives in its `commands/<name>.rs` file. Construct an
`EvidentRuntime` itself — that happens inside the `cmd_*`
function the dispatch lands in. Parse subcommand-specific flags
— each `cmd_*` parses its own. Hold state. Reach into runtime
internals (uses `commands::*` only). Print anything except the
usage banner on the bare-invocation / unknown-subcommand paths
— per-command output belongs in the command file.

The dispatch table — the `match args[0].as_str()` block — is
the file's only logic, and it's the only place subcommand names
are listed. Adding a subcommand means: add a `commands/<name>.rs`
file (with `pub fn cmd_<name>`), add a `pub mod <name>;` line
to `commands.rs`, add a match arm here. Three files, mechanical.

**Dependencies.** `commands` (the local module tree from the
binary's perspective; not the library), `std::process::ExitCode`.

**Generic-runtime analogue.** The `main.rs` / `main.go` /
`main.c` of any subcommand-organized CLI tool: `cargo`'s
`bin/cargo/main.rs`, `git.c`'s top-level `main`, `kubectl`'s
`cmd/kubectl/kubectl.go`. Always tiny — argv in, exit code out,
all real work behind the dispatch. A growing `main` is always
the wrong place to put logic.
