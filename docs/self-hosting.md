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

## Status — which transforms have been ported

| Transform | Rust | Evident pass | Faithful? | Notes |
|---|---|---|---|---|
| `pretty` (AST → String) | `portable/pretty.rs::RustPretty` | `stdlib/passes/pretty.ev` | **partial** | ASCII, non-recursive subset only — see [Gaps](#runtime-gaps-that-bound-a-string-pass) |
| `validate` (88 LOC) | `portable/validate.rs::RustValidate` | `stdlib/passes/validate.ev` | **faithful** | shared Rust walker + Evident-side classifier; pins `nm ∈ String` not `e ∈ Expr` to side-step the given-pinned-enum String-equality gap (see [Gaps](#runtime-gaps-that-bound-a-string-pass) and `examples/COUNTEREXAMPLES.md`) |
| `subscriptions` (176 LOC) | — | ⌛ | — | |
| `desugar` (273 LOC) | partial (`commands/desugar.rs`) | `stdlib/passes/desugar_passthrough.ev` | partial | pre-dates this seam; uses reflection path |
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

v1 uses a **per-port hand-written marshaler** (`portable/pretty.rs`'s
`encode_*` functions). It mirrors the private `*_to_value` family in
`translate/encode_ast.rs`; that surface is private (and `translate/` was
off-limits for this session), so the marshaler is duplicated locally,
which also keeps the port self-contained. A future cleanup can make the
`encode_ast.rs` mirror public and have every port share it (or add a
derive macro).

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
