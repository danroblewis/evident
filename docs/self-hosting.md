# Self-hosting transformations — the `portable/` swap interface

The Rust runtime (~31K LOC) is being progressively reduced as pure
transformations move into Evident passes under `stdlib/passes/*.ev`.
This document covers the **call-site seam**: how a Rust function gets a
swappable Evident implementation, how to port one, and which runtime
gaps currently bound what a pass can do.

> Related, but different concern: `docs/design/self-hosting-compiler-passes.md`
> (vision), `self-hosting-roadmap.md` (plan), and `self-hosting-status.md`
> (what ships) cover the **AST-reflection pass infrastructure** —
> encoding a whole `Program` as a Z3 datatype and running inference /
> lint rules over it. This doc is about the smaller, reusable
> **swap pattern** any pure Rust transform can adopt. The two compose:
> a ported pass uses the reflection encoders; the swap interface is how
> a caller chooses the Rust or the Evident backing.
>
> See also: [`design/self-hosting-inventory.md`](design/self-hosting-inventory.md)
> — every `runtime/src/**/*.rs` file classified into a 5-tier ladder
> (kernel / pure / tree-recursion / bounded-loop / unbounded), with
> the next ten ports named in order and a direct answer to "do we
> wait for FSM-with-loops?".

## Status — which transforms have been ported

| Transform | Rust | Evident pass | Faithful? | Notes |
|---|---|---|---|---|
| `pretty` (AST → String) | `portable/pretty.rs::RustPretty` | `stdlib/passes/pretty.ev` | **partial** | ASCII, non-recursive subset only — see [Gaps](#runtime-gaps-that-bound-a-string-pass) |
| `validate` (88 LOC) | `portable/validate.rs::RustValidate` | `stdlib/passes/validate.ev` | **faithful** | shared Rust walker + Evident-side classifier; pins `nm ∈ String` not `e ∈ Expr` to side-step the given-pinned-enum String-equality gap (see [Gaps](#runtime-gaps-that-bound-a-string-pass) and `examples/COUNTEREXAMPLES.md`) |
| `subscriptions` | **Evident-only** (`portable/subscriptions.rs::EvidentSubscriptions`) — Rust walk DELETED (session XX) | `stdlib/passes/subscriptions.ev` | **full, sole impl** | Cut over in session XX: the canonical `subscriptions::world_access_sets` Rust walk is gone; the scheduler computes every claim's `(reads, writes)` through the stack-FSM via `portable::subscriptions::access_sets` (cached engine, WW resolver). Whole walk is a stack-FSM fed by the SHARED marshaler (UU); only the `world.`/`world_next.` prefix split stays in Rust (no substring op in Evident). Pinned per-claim expectations on the corpus incl. Mario in `runtime/tests/subscriptions_correctness.rs` |
| `desugar` (273 LOC) | partial (`commands/desugar.rs`) | `stdlib/passes/desugar_passthrough.ev` | partial | pre-dates this seam; uses reflection path |
| `desugar_seq_concat` (the `++` flattening half of `desugar`) | `portable/desugar.rs::RustDesugar` (wraps the canonical, the production path) | `stdlib/passes/desugar.ev` (`desugar_gather` + `desugar_flatten` stack-FSMs) | **faithful, NOT cut over** | Session PORT-desugar. The recursive kernels self-host as stack-FSMs over the SHARED marshaler (UU) via `run_nested`: `desugar_gather` folds the body → `name ↦ ⟨items⟩` bindings; `desugar_flatten` resolves a `Concat` spine → a chunk stream (`FLitItem` verbatim items + `FRef` identifier refs). The structural pre-order `rewrite` walk stays in Rust, and the string-keyed `FRef` lookup stays in Rust (Evident's enum-payload String equality is unreliable — #18). Byte-identical to the canonical on the corpus + a synthetic battery (`runtime/tests/desugar_equivalence.rs`). **Kept Rust as the production path** — two gaps block cutover: (1) the other desugar pass `unify_world_syntax` rewrites identifier strings by prefix-strip, which Evident can't express at all (no substring op); (2) returning the whole rebuilt body through the marshaler is byte-lossy in general (`SchemaDecl.param_count`, nested `MatchPattern` binds). See [Gaps](#runtime-gaps-that-bound-an-ast-returning-pass). |
| `generics` (256 LOC) | — | ⌛ | — | |
| `inject` (588 LOC) | — | ⌛ | — | biggest |

"Faithful" = the Evident impl produces byte-identical output to the Rust
impl on the test fixtures.

## The pattern

Each swappable function gets one module under `runtime/src/portable/`.
It owns three things:

1. **A typed trait** — the function's Rust-level signature, independent
   of which impl backs it. (`PrettyImpl { fn expr(&self, &Expr) -> String;
   fn body_item(&self, &BodyItem) -> String; }`.)
2. **The Rust impl** — the original native code, the default. Fast,
   total, always correct. (`RustPretty`.)
3. **The Evident impl** — owns an `EvidentRuntime` with the stdlib pass
   loaded; marshals the Rust input into a `Value`, runs `rt.query`,
   decodes the output binding. (`EvidentPretty`.)

Every impl is also `Portable` (an `impl_name()` for tracing).

### Selection: by construction, not a registry

`EvidentRuntime::new()` is *not* modified to hold an impl slot. The impls
are standalone: a caller — or a cross-validation test — builds a
`RustPretty` or an `EvidentPretty` and calls the trait method. Each
module exposes `default_impl() -> Box<dyn Trait>` selected by an env var
(`EVIDENT_PRETTY_IMPL=rust|evident`), defaulting to Rust.

Rationale: standalone impls keep the seam side-effect-free (the choice
can't leak across queries), and they avoid touching `runtime/` internals.
A registry slot on `EvidentRuntime` is a viable future refinement once a
pass needs to be the *production* default — but until a pass is fully
faithful, the Rust impl stays the default and the Evident impl is
exercised by tests and opt-in env var.

When `default_impl()` selects the Evident backing, it locates the pass
file's `stdlib/` via the one resolver `stdlib_path::stdlib_dir()` (see
[`docs/design/stdlib-resolution.md`](design/stdlib-resolution.md)) — set
`EVIDENT_STDLIB` to override, otherwise the dev tree resolves with no
config. This is the robust stdlib location that session VV flagged as the
prerequisite for any production Evident-pass cutover (an installed binary
with no source tree must still find its stdlib).

### Marshaling: Rust value → `Value::Enum` → pass → `Value`

The pass pattern-matches on an Evident-side mirror of the AST defined in
`stdlib/ast.ev` (enums `Expr`, `BodyItem`, `SchemaDecl`, `Pins`, …). The
Rust side encodes its input as a `Value::Enum` tree whose enum/variant
names match those enums exactly, pins it as a `given`, and reads the
output from `QueryResult.bindings["out"]`.

```rust
// EvidentPretty::render — the whole seam in one place
let mut given = HashMap::new();
given.insert("item".to_string(), encode_body_item(item)); // → Value::Enum
let qr = self.rt.query("Pretty", &given)?;
match qr.bindings.get("out") { Some(Value::Str(s)) => s.clone(), _ => … }
```

The Evident claim declares its inputs as parameters and its outputs as
free vars:

```evident
claim Pretty
    item ∈ BodyItem        -- input, pinned via `given`
    out  ∈ String          -- output, read from bindings
    out = match item …
```

**One shared marshaler (session UU).** The `*_to_value` family in
`translate/encode_ast.rs` is `pub` and is THE marshaler every port reuses
— re-exported as `translate::ast_encoder::{program_to_value,
schema_decl_to_value, body_item_to_value, expr_to_value, …}`. Its read
twin is `translate::ast_decoder::{decode_list, decode_str, decode_program,
…}`. A port no longer hand-rolls an `encode_*`: it calls the shared one.

Why this matters: QQ measured that a per-port hand-written encoder is a
recursive AST traversal *isomorphic to the walk it deletes*, so each port
re-paid that "marshaling tax" and net Rust never went down (you traded
"walk in Rust" for "encode-to-Value in Rust"). Sharing the encoder pays
the tax **once**. After UU the *marginal* port is:

```
+ stdlib/passes/<name>.ev   (the Evident pass — the analysis/transform)
− the Rust walk it replaces  (deleted)
+ ~3 lines of Rust glue:     encode → run → decode
```

The 3-line glue, concretely (subscriptions' shim):

```rust
// encode: shared marshaler, ast.rs → Value::Enum (cons-list shape)
let seed = work_node("WBody", body_item_to_value(item));
// run:    drive the pass FSM to halt as a value
let final_state = run_nested(&self.rt, "subscriptions_walk", seed, MAX)?;
// decode: shared cons-list reader, Value → Vec<String>
let names = decode_list(&fields[0], "NameList", "NameNil", "NameCons", decode_str)?;
```

**List shape**: `*_to_value` encodes list fields as poppable Cons enums
(`BodyItemList`, `ExprList`, …), not `Seq(T)` — a `Seq` has no in-step
pop (COUNTEREXAMPLES #19a), so the Cons shape is what a stack-FSM walk
consumes directly. A pass that pins the AST as a `given` over
`stdlib/ast.ev`'s `Seq`-shaped enums uses the `encode_*` Datatype family
instead (reflection, `literal_types.ev`).

**FUTURE WORK** (durable follow-up, NOT built in UU): generate the
`*_to_value` / `decode_*` family from the `ast.rs` types with a **derive
macro**, so the marshaler can never drift from the AST shape by hand. UU
made the surface shared; the derive macro makes it un-droppable.

### Cost

The Evident path runs through `EvidentRuntime::query`, which JIT-caches
the compiled claim after the first call. Steady-state cost is a JIT
function call (~µs) plus marshaling — not a full Z3 solve. Loading the
pass file is the one-time construction cost, so **hold the Evident impl
across calls** rather than rebuilding it per call. `pretty` is not on a
hot path, so this is comfortable; for passes that run at every file load
(`inject`, `desugar`) the cached-function cost is what to budget for.

## To port a Rust function

1. Identify the signature (input types, output type).
2. Create `runtime/src/portable/<name>.rs` with the trait + `Rust<Name>`
   impl (move or wrap the existing native code). Add `pub mod <name>;`
   to `portable/mod.rs`.
3. Write `stdlib/passes/<name>.ev`: a claim taking the input as a
   parameter and exposing the output as a free var. Keep each claim
   **flat** (a single `match`/equality) — Evident can't recurse (below).
4. Add `Evident<Name>` to the portable module: marshal input → `Value`,
   `rt.query`, decode output. Reuse `encode_*` shapes from
   `stdlib/ast.ev`.
5. Add `runtime/tests/<name>_equivalence.rs`: assert
   `rust(x) == evident(x)` on representative inputs; pin known
   divergences so a future gap-fix surfaces them.
6. Make the old module a thin wrapper over `portable::<name>::Rust<Name>`
   (as `pretty.rs` now is) so existing call sites are unchanged.
7. Update the table at the top of this doc.

## Runtime gaps that bound a string pass

Porting `pretty` (a pure `AST → String` recursion) surfaced two
fundamental limits. Both must be fixed in `translate/` + `functionize/`
before an AST→String (or AST→AST) pass can be *fully* faithful. Until
then, a pass is faithful only on the **ASCII, non-recursive subset**.
(Also logged in `examples/COUNTEREXAMPLES.md`.)

> **See also:** [`design/loop-functionizer.md`](design/loop-functionizer.md)
> is the unlock for the blocked tree-walk ports below. It routes around
> the recursion gap (Gap #1) *without* adding recursion to the constraint
> language — the walk becomes a loop-functionized FSM over an explicit
> work-stack, which finally lets `subscriptions` / `validate` / `pretty`
> move their *walk* (not just the leaf classifier) into Evident and
> delete the Rust walk + its `portable/` duplicate. That's the port shape
> that makes net Rust LOC go *down*.

### 1. Recursive claims don't constrain their outputs

A claim cannot recurse over a nested `Expr` tree of unknown depth.
There is a bounded-inlining mechanism (`translate/inline/recursion.rs`,
depth-capped at `EVIDENT_MAX_INLINE_DEPTH=64`), but the inlined frames'
outputs are left **unconstrained** — Z3 fills free values, so the result
comes back as garbage (both correct and wrong outputs are SAT).
Additionally, a claim call nested inside an expression
(`out = pretty(l) ++ …`) is **silently dropped** (`translate/inline/walk.rs`).
There is no `define-fun-rec`, no fold/catamorphism, no string-fold.

Consequence: only leaf / flat shapes render. Anything with sub-`Expr`s
(`EBinary`, `ECall`, `ESetLit`, quantifiers, mapping lists) cannot.
See the unchecked acceptance criteria in
`docs/plans/03-language-prereqs/01-recursive-claims.md`.

### 2. Non-ASCII string literals mangle through Z3

`Z3Str::from_str` treats a Rust `&str`'s UTF-8 bytes as a byte-sequence
of Z3 characters. A source literal `" ∈ "` comes back as
`\u{e2}\u{88}\u{88}` (JIT path — raw escape text) or `â\u{88}\u{88}`
(slow path — per-byte codepoints). Neither recovers `∈`. So a string
pass can only faithfully emit **ASCII**; every operator glyph `pretty.rs`
restores (`∈ ∀ ⇒ ∧ ¬ ≤ ≥ ↦ ⟨ ⟩ …`) is lost.

(The `given` round-trip of a `Value::Str(" ∈ ")` *appears* to work, but
only because the JIT identity-short-circuits and returns the input
`Value` unchanged — it isn't real Z3 Unicode support.)

### 3. (minor) JIT mishandles a `Bool` payload nested in an enum given

`match e { EBool(b) ⇒ (b ? "true" : "false") }` returns `"false"` for
both `true` and `false` under the JIT, but is correct on the slow path
(`EVIDENT_FUNCTIONIZE=0`). A nested `Bool` payload in an enum `given`
isn't threaded through the functionizer's match→ternary codegen. Bool
rendering is therefore excluded from the faithful subset rather than
shipped as a JIT-incorrect arm.

### 4. String payload extracted from a given-pinned enum loses equality

Surfaced by porting `validate`. The natural shape — pin an `e ∈ Expr`,
destructure `ECall(nm, _)`, compare `nm = "FFICall"` — evaluates the
equality to `false` on both JIT and slow paths, even when the bytes
match. The destructured `nm` doesn't byte-compare against a source
literal of the same value when `e` was pinned via `given` from a Rust
`Value::Enum { fields: [Value::Str("FFICall"), …] }`.

Workaround used in `stdlib/passes/validate.ev`: have the shim
extract the call name on the Rust side and pin `nm ∈ String` directly.
The pass still owns the decision (`nm ∈ {FFICall, FFIOpen, FFILookup,
LibCall}`); only the recognizer-vs-comparison choice changes. Logged
in `examples/COUNTEREXAMPLES.md` (gap class: "given-pinned-enum
String-equality").

The constructed-in-source form works correctly — `e = ECall("LibCall",
⟨⟩) ; ValidateExpr (e ↦ e, …)` from an `.ev` file produces the
expected `out = "LibCall"`. So this is specifically a `given` ⇄ match
extraction failure, not a fundamental string-comparison issue.

## What `stdlib/passes/pretty.ev` reproduces today

Faithful (byte-identical to `pretty.rs`):

- `PrettyExpr`: `EIdentifier(n)` → `n`
- `Pretty`: `BIPassthrough(c)` → `..c`; `BIClaimCall(name, ⟨⟩)` → `name`
  (empty mappings); `BIConstraint(EIdentifier)` → delegated to
  `PrettyExpr` by the Rust shim (mirrors `pretty.rs`, whose `body_item`
  Constraint arm calls `expr`).

Everything else renders to an ASCII sentinel (`<unsupported-…>`); the
equivalence test treats only the faithful shapes as authoritative and
pins the rest as known divergences.

## What `stdlib/passes/subscriptions.ev` reproduces today

**This is now the SOLE subscriptions implementation** (session XX cut over
to Evident-only; the canonical Rust `subscriptions::world_access_sets` walk
is deleted). The scheduler computes every FSM-shaped claim's `(reads,
writes)` through this pass — including the Mario demo (three FSMs, ~30
fields across read/write sets combined). The pinned per-claim expectations
in `runtime/tests/subscriptions_correctness.rs` were captured from the Rust
walk before its deletion and now stand on their own.

The WHOLE walk runs in Evident, not just a leaf classifier.
`subscriptions.ev` is a stack-FSM, `subscriptions_walk`, whose state
carries a **work stack** (poppable `Work`-wrapped AST nodes still to
visit) plus a **reachable-identifier accumulator**. Each tick pops the
top node, dispatches on its shape, and either folds it (an
`EIdentifier`), drops it (a literal/inert node), or pushes its children —
exactly the test_37 stack-FSM shape, driven to a drained-stack halt by
`run(...)`. The traversal control and the accumulation live in the pass.

As of session UU the FSM walks the **full canonical AST** — the same
`Expr`/`BodyItem`/`Pins`/… shapes `stdlib/ast.ev` defines (list fields as
poppable Cons enums) — because it is fed the output of the ONE SHARED
marshaler. There is **no bespoke `WNode` encoder anymore**.

The Rust shim (`portable/subscriptions.rs`) does no tree walk and no
hand-rolled encoding. It:

1. Encodes each top-level body item with the SHARED marshaler
   `ast_encoder::body_item_to_value` (ast.rs → `Value::Enum`), wrapped as
   the FSM's unified `Work::WBody(BodyItem)` node. No per-pass encoder.
2. Drives `subscriptions_walk` over each encoded item via
   `effect_loop::run_nested`, one item at a time so the per-tick state
   stays shallow (a whole-body seed makes the per-tick datatype
   marshaling O(N²); per-item keeps it O(N) — the difference between a
   sub-second and a multi-minute walk of Mario's `game`).
3. Decodes the final `SWDone(NameList)` cons-list with the SHARED reader
   `ast_decoder::decode_list`, then classifies each raw identifier (see
   below). Dedup into HashSets makes element order irrelevant; reads/writes
   is a set union over items, so per-item-then-union is identical to a
   single whole-body walk.

### Classification stays in Rust (no substring op)

The FSM owns the traversal but NOT the `world.`/`world_next.`
classification: that needs `strip_prefix` + `first_segment`, and Evident
has no substring/prefix builtin. So the FSM emits the RAW dotted
identifier strings it reaches and the shim's `classify` does the prefix
split — a few lines, mirroring the canonical `walk_expr` `Identifier` arm
1:1. (QQ kept classification in the FSM only by pre-splitting identifiers
into segments inside its bespoke encoder — exactly the per-pass encoder
UU removes. Sharing the marshaler means the encoder no longer pre-splits,
so the unavoidable string op moves back to the ~10-line Rust classifier.
A future `split`/`prefix` operator in the language would let it move back
into the pass.)

### What this port now costs (the LOC inversion, finally)

QQ measured this port as net-flat Rust: a faithful AST→`Value` encoder is
a recursive traversal isomorphic to the walk it deletes, so each port
re-paid that marshaling tax. UU pays it once — the `*_to_value` family is
shared — so the shim dropped **333 → 228 LOC** (the ~149-line bespoke
`WNode` encoder + cons-list decoder block deleted, replaced by a 3-line
marshaler call + a ~10-line prefix classifier). The *marginal* next port
is now `+Evident pass, −Rust walk, +~3 lines of glue`. Session XX took the
final step: it **deleted** the canonical `subscriptions::world_access_sets`
walk and routed the scheduler through `EvidentSubscriptions`, so the port
is net-negative on real logic, not just test code (the equivalence test
went away too). The scheduler's production entry is now the free
`portable::subscriptions::access_sets`, backed by a per-thread cached
engine that loads the pass via the WW stdlib resolver.

### No bootstrap cycle

Computing subscriptions for the user's FSMs runs `subscriptions_walk` via
`effect_loop::run_nested` — the tier-3 blocking interpreter, which drives a
single FSM with per-tick Z3 solves and **never** calls `access_sets` or any
scheduler-level subscription inference. And `subscriptions_walk` reads no
`world.X` (its state is the plain `SW` stack machine), so its own
access-set is empty. The pass that computes subscriptions does not itself
need subscriptions — the recursion terminates. See
`subscriptions_correctness.rs::bootstrap_*`.

### Correctness test corpus

`runtime/tests/subscriptions_correctness.rs` pins the expected `(reads,
writes)` for every FSM-shaped, world-touching claim in:

```
examples/test_09_two_fsms.ev          examples/test_25_per_component_jit.ev
examples/test_14_stdin.ev             examples/test_26_value_cache.ev
examples/test_15_signal.ev            examples/test_30_jit_gap_closures.ev
examples/test_18_reflection.ev        examples/test_31_symbolic_regression.ev
                                      examples/test_32_llm_functionizer.ev
                                      examples/test_21_mario/main.ev
```

(Surveyed by `grep -l 'world\.\|world_next\.' examples/test_*.ev`.) These
are direct expectations, not a comparison against a deleted oracle. The
Mario claims (`game` major writer, `keyboard` input writer, `display`
reader) codify the demo's shape so a behavioural regression surfaces here.

## What `stdlib/passes/validate.ev` reproduces today

Fully faithful — `EvidentValidate` and `RustValidate` produce
byte-identical diagnostics for every `SchemaDecl` in the corpus
(every example in `examples/test_*.ev`, plus the synthetic
violations the equivalence test constructs across kind labels,
banned call names, and nesting positions).

The trick is that the body walk lives in Rust on **both** impls
(`portable/validate.rs::find_ffi_call` mirrors the canonical
`runtime/src/runtime/validate.rs::find_ffi_call` 1:1); the impls
only differ in the per-Call classifier. `RustValidate` uses a
native `match name { "FFICall" => ... }`; `EvidentValidate` calls
`ValidateExpr(nm)` in the pass. The decision logic — what counts
as a banned name — lives in the Evident pass, which is the only
piece that moves between impls.

This split is intentional: the recursion gap (Gap #1) blocks an
Evident pass from walking the Expr tree itself, but it doesn't
block the much smaller "is this name banned?" decision. Porting
that piece keeps the seam useful even before the recursion gap is
closed, and the equivalence test pins the behaviour byte-for-byte
so a future gap fix can promote the walk into Evident without
silently changing the diagnostic surface.

## What `stdlib/passes/desugar.ev` reproduces today

`desugar_seq_concat` — the `++` (Seq concat) flattening half of the
canonical `desugar` pass (`runtime/src/runtime/desugar.rs`). Unlike
`subscriptions` (which FOLDS a body to name-sets) this pass **returns a
rewritten AST**, which is what makes it the interesting case: it's the
first port that consumes AND produces composite AST through the shared
marshaler.

Two recursive kernels self-host as stack-FSMs driven by `run_nested`
(tier-3), over the SHARED marshaler (UU):

  * `desugar_gather` — folds a `BodyItemList` into an `Assoc` cons-list of
    `name ↦ ⟨items⟩` bindings (the canonical pass-1). No string equality
    (it matches `BIConstraint(EBinary(OpEq, EIdentifier, ESeqLit))`
    structurally), so it self-hosts cleanly.
  * `desugar_flatten` — resolves a `Concat` spine into an ordered CHUNK
    stream: `FLitItem(e)` for a `⟨…⟩` operand's items (verbatim, matching
    `flatten`'s `items.clone()`), `FRef(name)` for an identifier operand,
    `FFail` for any other shape (matching `flatten`'s `_ => None`). This is
    the genuine recursive fold — the Concat tree walk + the left-to-right
    item ordering live in the FSM.

`EvidentDesugar` (`portable/desugar.rs`) drives them, and is byte-identical
to `RustDesugar` (which wraps the canonical pass verbatim) on the corpus +
a synthetic battery (`runtime/tests/desugar_equivalence.rs`).

### What stays in Rust, and why (the honest split)

The shim keeps two pieces in Rust:

  1. **The structural pre-order `rewrite` walk** — which `Expr` nodes to
     visit, where to splice the flattened `SeqLit`, recursion into
     subclaims. Mechanical plumbing, a faithful copy of the canonical
     `rewrite` (the same shared-walk discipline `validate` uses).
  2. **The string-keyed `FRef` lookup** — resolving `FRef(name)` to its
     `⟨items⟩` against the gathered map. This is Rust because Evident's
     **String equality on enum-extracted payloads is unreliable** (Gap #4 /
     `COUNTEREXAMPLES.md` #18): two strings both destructured from a
     given-pinned enum don't compare byte-equal, so an in-FSM
     `seq_lits.get(name)` mis-fires (it silently resolves every identifier
     to the first binding). The FSM therefore emits `FRef(name)` without
     comparing, and the shim does the lookup — the same "Evident owns the
     recursion, Rust owns the string leaf" split as `subscriptions`
     (FSM walk → Rust prefix-classify).

## Runtime gaps that bound an AST-returning pass

Porting `desugar_seq_concat` surfaced the limits of self-hosting a pass
that *returns* rewritten AST (not just a fold). These are why
`desugar` is **self-hosted + equivalence-proven but NOT cut over** — the
canonical Rust pass stays the production load path:

### A. `unify_world_syntax` can't be self-hosted at all (no substring op)

The canonical `desugar` runs **two** rewrites. The second,
`unify_world_syntax`, rewrites identifier strings by prefix manipulation:
`"_world.X" → "world.X"` and `"world.X" → "world_next.X"` (via Rust
`strip_prefix` + `format!`). Evident has **no substring / prefix / runtime
string-construction operator** — the same wall `subscriptions` hit for its
`world.`-prefix classification, except there it was a leaf decision punted
to a few Rust lines, whereas here it is the *entire* transform (a per-leaf
string REWRITE, not a yes/no test). There is no honest way to express it in
Evident today, so it stays in Rust in full.

### B. Returning the whole rebuilt body through the marshaler is byte-lossy

A full all-Evident rewrite would have the FSM return the entire rewritten
body, decoded back via the shared marshaler. That round-trip is **not
byte-identical in general**, by two documented marshaler limitations:

  * **`SchemaDecl.param_count` has no marshaler slot.** `MakeSchemaDecl`
    carries `(Keyword, String, BodyItemList)` only; `decode_schema_decl`
    reconstructs `param_count: 0` (and `external: false`, `type_params:
    []`). So any subclaim (a `BISubclaim(SchemaDecl)` in the body) that has
    first-line params round-trips to `param_count = 0` — a silent change to
    a field later passes rely on. (`EvidentDesugar` sidesteps this by
    holding subclaims as Rust `SchemaDecl`s and recursing into them, never
    round-tripping them — but a *whole-body-return* design couldn't.)
  * **Nested `MatchPattern` constructor sub-patterns collapse.** A bind
    list can only carry `BindName` / `BindWildcard`, so a nested-ctor
    pattern (`Cons(Cons(x), y)`) reflects lossily as `BindWildcard`
    (`encode_ast.rs` / `decode_ast.rs`). The current corpus uses only flat
    patterns so the corpus round-trips losslessly, but the general case
    doesn't.

Because of A and B, a faithful *cutover* (delete the Rust rewrite, route
`load.rs` through Evident) isn't possible. The right outcome — per the
honest-fallback policy — is to self-host the faithfully-expressible kernels,
equivalence-prove them, and keep the Rust pass. Closing Gap #4 (enum-payload
String equality) would let `desugar_flatten` do its own `FRef` lookup;
closing A (a substring/prefix primitive) + B (a `param_count` marshaler slot
+ deep `MatchPattern` binds) would together unblock a real cutover.

### C. Runtime is unaffected either way (load-time pass)

`desugar` runs once per schema at **load**, never on the per-tick scheduler
hot path (it isn't called from `effect_loop`). So even the Evident-backed
impl moves only one-time load cost — steady-state per-tick runtime is
untouched. Production stays on the Rust pass regardless, so there is no
runtime delta at all; the Evident impl is reachable only via
`EVIDENT_DESUGAR_IMPL=evident` and the equivalence test. (Per the
runtime-over-setup priority, even a cutover here would have been a
setup-only cost — but the faithfulness gaps above block it independently.)
