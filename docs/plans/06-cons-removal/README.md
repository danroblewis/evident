# Phase 6: Remove Cons/Nil linked-list structures

## Goal

Replace every Cons/Nil enum pair in the runtime with the language's
built-in `Seq` (ordered) or `Set` (unordered) — chosen per use case
based on whether order is semantically meaningful. Delete every
recursive walker built to traverse Cons chains; consumers iterate
Seqs by index and Sets by element directly.

After this phase: no Cons/Nil pairs anywhere in `stdlib/` or the
runtime; no `decode_*_list` recursive functions; no shared
linked-list types like `StrList` / `IntList` / `PackedFieldList` —
each FFI binding declares its own argument shape.

## Background

The Cons/Nil pattern entered the codebase early as a side-effect of
this language's tangential relation to functional programming. We
chose Cons; therefore we needed recursive walkers to consume it.
Neither was ever necessary — the language has Set and Seq as
first-class set-theoretic constructs with direct access
(`s[i]`, `#s`, `∀ i ∈ {0..#s}`, set membership and iteration).

Cons-shaped enums in scope today:

**FFI / runtime** (`stdlib/runtime.ev`):
- `EffectList` — emitted effects per tick
- `ResultList` — results threaded back as `last_results`
- `ArgList` — FFI call arguments
- `StrList`, `IntList`, `PackedFieldList` — nested FFI buffers

**AST** (`stdlib/ast.ev`):
- `StringList`, `ExprList`, `BodyItemList`, `BindList`,
  `MatchArmList`, `EnumVariantList`, `EnumFieldList`,
  `MappingList`, `SchemaList`, `EnumDeclList`

## Seq vs Set: the decision rule

**Per use-case, not blanket.** For each Cons enum (and each FFI
call's nested-list argument), ask: does the consumer care about
order?

- `EffectList`: `Println` before `Exit` doesn't commute → **Seq**.
- `ResultList`: position-aligned with the effects that produced
  them → **Seq**.
- `ArgList` for one C call: arguments are positional → **Seq**.
- AST lists (`BodyItemList`, `ExprList`, etc.): traversal order
  matters for desugar / inference passes → **Seq**.
- Nested FFI list arguments (`StrList`, `IntList`,
  `PackedFieldList` today; per-binding shapes after migration):
  **decided per C call**. Most are Seq (`glShaderSource` strings,
  vertex stream, `SDL_Rect` x/y/w/h) because most C surface is
  positional. Genuinely set-shaped cases — capability flags,
  handler registrations, attribute sets where order is irrelevant
  — become Set.

When a Set is marshalled to C, the dispatcher picks a buffer order
(the ABI wants a contiguous `T*` regardless). Stable within one
call; the program doesn't get to depend on which order — that's
what Set is *for*.

## Structural change: no shared linked-list types

`StrList` / `IntList` / `PackedFieldList` exist today because we
needed a Cons shape per element type. After migration they
disappear entirely. Each FFI binding declares its argument shape
directly in its own signature:

```evident
claim gl_shader_source(shader ∈ Int,
                       sources ∈ Seq(String),
                       out ∈ Effect)

claim gl_draw_arrays(verts ∈ Seq(Vertex),
                     mode ∈ Int,
                     out ∈ Effect)

claim register_handlers(handlers ∈ Set(Handler),
                        out ∈ Effect)
```

No `StrList`. No `IntList`. No `PackedFieldList`. Each call's
signature is honest about what shape it wants.

## Prerequisite: Seq + Set runtime parity

Seq today is a Z3-solver-facing abstraction (array + length
variable). Set similarly exists at the constraint layer. Neither
has a dispatch-time decode path to Rust `Vec` / `HashSet`. That
gap is the only piece of real engineering in this phase;
everything after it is mechanical.

What's needed:

1. **Z3-model → `Vec<Value>`** decoder for Seq variables.
2. **Z3-model → `HashSet<Value>`** decoder for Set variables.
3. **Literal-construction syntax** at the language layer:
   `Seq(a, b, c)`, `Set{a, b, c}` (or whatever shape you prefer —
   see open question below).
4. **Runtime equality** preserved through decode (already works at
   the Z3 layer; verify it holds once decoded).

What's **not** needed:

- Pattern-match destructuring like `match s case Cons(h, t)`.
  That's Cons-thinking in Seq clothes. Consumers use direct
  access — `s[i]`, `#s`, `∀ i ∈ {0..#s}`, set iteration.
- Recursive walkers. None of the migrated consumers should
  recurse; they loop or iterate.
- A "two-runtime" coexistence period. There's no external user
  base; migrate per-phase, keep tests green, don't preserve
  back-compat.

## Phases

### 6.1 — Seq + Set runtime parity

Build the dispatch-time decoders, literal constructors, and
verify roundtrip (declare → constrain → solve → decode →
re-encode → constrain) works for `Seq(Int)`, `Seq(String)`,
`Seq(Enum)`, `Set(Int)`, `Set(Enum)`. After this phase nothing
external has changed; the rest of the runtime still uses Cons.

### 6.2 — FFI argument shapes (per-binding rewrite)

This is the user-visible payoff: six-deep `ArgCons(ArgInt(42),
ArgCons(ArgStr("foo"), …))` chains collapse to flat `Seq(...)` or
`Set{...}` literals.

For each FFI binding (`packages/sdl/`, `packages/gl/`,
`stdlib/shell.ev`, `stdlib/posix.ev`, …):

1. Decide Seq vs Set for each list-shaped argument.
2. Rewrite the binding's signature to use `Seq(T)` / `Set(T)`
   directly — no shared linked-list type.
3. Update the FFI marshaller in `runtime/src/ffi.rs` to decode
   Seq/Set arguments to the libffi buffer shape (buffer-order
   picked by dispatcher for Set).
4. Update call sites.

Delete `ArgList`, `StrList`, `IntList`, `PackedFieldList` and
their constructors from `stdlib/runtime.ev`. Delete
`decode_arg_list` and friends from runtime.

### 6.3 — ResultList → `Seq(Result)`

Position-aligned with effects, so straightforwardly Seq. The
runtime pins `last_results` as a Z3 Datatype each tick; switch to
pinning a Seq variable. Encode path in `effect_loop.rs` rewrites.
User FSMs' `match last_results case ResCons(r, _) ⇒ …` patterns
become indexed access (`last_results[0]`) — and many will simplify
because they were Cons-walking just to get the first result.

Delete `ResultList` / `ResCons` / `ResNil` from
`stdlib/runtime.ev`. Delete the recursive decoder from runtime.

### 6.4 — EffectList → `Seq(Effect)`; decide Effect::Seq fate

The dispatcher's hot path; do it after FFI args and ResultList so
the patterns are established.

**Open design decision**: does `Effect::Seq(EffectList)` survive?
Today it's a metaeffect that wraps an EffectList to run as one
atomic solver step with `ArgPriorResult` cross-refs. If top-level
effects are already `Seq(Effect)`, then "this is one atomic unit"
becomes a Seq-level concept, not a separate effect wrapper.

- **Collapse**: `Effect::Seq` disappears. The atomic-batch idea
  becomes "the top-level emitted Seq is one atomic unit, and
  `ArgPriorResult` references are Seq-relative."
- **Keep**: `Effect::Seq(Seq(Effect))` exists for explicit
  batching when the top-level emission shape doesn't fit.

Recommendation: collapse. The top-level FSM emission IS the
atomic unit; we don't need a wrapper effect to express that.
Defer the final call to when this phase starts.

Delete `EffectList` / `EffCons` / `EffNil`. Delete
`decode_effect_list`.

### 6.5 — AST lists → `Seq(...)`

Ten Cons enums in `stdlib/ast.ev`. Lower risk because they only
touch desugar / inference / self-hosted pass machinery, not the
runtime hot loop. Can be batched into one task or split per
list type.

Delete `StringList`, `ExprList`, `BodyItemList`, `BindList`,
`MatchArmList`, `EnumVariantList`, `EnumFieldList`,
`MappingList`, `SchemaList`, `EnumDeclList` from
`stdlib/ast.ev`. Delete the corresponding recursive decoders /
encoders in runtime.

### 6.6 — Sugar + cleanup

**Open design decision**: ⟨...⟩ sugar today lowers to Cons.
After Phases 6.1–6.5, no Cons type exists for it to target.
Three options:

- **Retarget to Seq literals.** `⟨a, b, c⟩` becomes `Seq(a, b, c)`.
  Least disruptive — existing demos keep working with the sugar.
- **Drop the sugar.** Write `Seq(a, b, c)` and `Set{a, b, c}`
  explicitly everywhere. More uniform with how other types
  instantiate; readable type at every call site.
- **Retarget AND add Set sugar.** `⟨a, b, c⟩` → Seq;
  introduce a Set literal shape (`{a, b, c}` or similar) for
  parity.

Recommendation: retarget to Seq, no Set sugar — Set construction
is rarer and the explicit `Set{...}` form keeps the type visible
where it matters.

After this phase: `lexer.rs`, `parser.rs` no longer emit Cons
constructors anywhere. Verify with a grep that no `*Cons` / `*Nil`
appears in the runtime tree.

## Acceptance

- `grep -rn '\(Cons\|Nil\)' stdlib/ runtime/src/` returns nothing
  (modulo unrelated identifiers like `connect`).
- No `decode_*_list` recursive function remains in `runtime/src/`.
- `./test.sh` passes (Rust unit + integration + conformance).
- `./test.sh --examples` passes; visual demos still render.
- `runtime/tests/demos.rs` `EXPECTATIONS` table unchanged.

## Estimate

Phase 6.1 is the only phase with real design uncertainty (the
Seq/Set Z3-decode path, literal constructor shape). Probably one
focused day. Phases 6.2–6.5 are each a half-day to a day of
mechanical rewrites. Phase 6.6 is a few hours.

## Out of scope

- Set as the answer for sequential structures — none of the Cons
  enums in the current codebase need it. Per-binding FFI args
  are the only place where the Seq/Set choice is open, and only
  for arguments where the C contract genuinely doesn't care
  about order.
- Coexistence period or migration shims — direct migration
  per-phase, no back-compat.
- Reworking `Effect::Seq`'s `ArgPriorResult` semantics. We may
  collapse the wrapper but the cross-reference mechanism stays.

## Open questions (to resolve before each phase starts)

1. **Phase 6.1**: literal constructor syntax. `Seq(a, b, c)` and
   `Set{a, b, c}` — or something else?
2. **Phase 6.2**: any specific FFI calls in the current
   `packages/` or `stdlib/` tree where the nested list is
   genuinely set-shaped? Default assumption: all current cases
   are Seq.
3. **Phase 6.4**: collapse `Effect::Seq` or keep it as
   `Effect::Seq(Seq(Effect))`?
4. **Phase 6.6**: retarget `⟨...⟩` sugar to Seq, or drop it?
