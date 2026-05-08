# SMT-LIB as Evident's Intermediate Representation

## The Framing

Evident sits at a specific point in the constraint-language landscape: a source language that compiles to a constraint system, hands it to Z3, and uses the model. The "constraint system" we hand Z3 today is a Rust in-memory representation built by `runtime-rust/src/translate/`. Each translator call produces fresh Z3 expressions; the constraint system never escapes the process.

That has worked, but it has a cost: every consumer that wants to see Evident's constraint system has to be linked into the same Rust binary. There is no on-disk artifact that says "here is the meaning of this Evident program in a portable form." Other tools — Z3 itself can't ingest our representation, CVC5 has never heard of us, the SMT competition's benchmark library can't run our examples through other solvers — are all walled off.

The fix is to adopt an existing standard rather than invent a new one. **SMT-LIB v2** is the lingua franca of SMT-backed languages. Z3, CVC5, Yices, MathSAT, Boolector all consume it natively. Adopting it as Evident's serialized HIR (high-level intermediate representation) costs us almost nothing — we already produce something semantically equivalent in-memory — and it opens up the whole ecosystem.

---

## What Already Exists Conceptually

The Evident pipeline today, viewed as compiler stages:

```
Evident source
  → AST (parser/src/, runtime-rust/src/parser.rs)
  → Z3 expression tree (runtime-rust/src/translate/)
  → in-memory Z3 solve / sample
```

The middle layer is the implicit IR. It's well-typed and well-defined, but it's not portable — its data structures live in our process's memory.

What we want:

```
Evident source
  → AST
  → SMT-LIB v2 text (our HIR — serializable, standard)
  → choose: Z3 / CVC5 / our function-set LIR / a sampler / etc.
```

The on-disk SMT-LIB form replaces the in-memory Z3 tree as the canonical "the program said this" artifact. The runtime can still build Z3 trees directly when we control both sides (fast path); the SMT-LIB form is for everything that crosses a process or tool boundary.

---

## What HIR Covers (and Doesn't)

SMT-LIB v2 is broad but not Evident-shaped. The mapping is mostly clean for the constraint-language subset of Evident; some features have no SMT-LIB analogue and stay Evident-only.

### Mapping table — Evident → SMT-LIB

| Evident | SMT-LIB | Notes |
|---|---|---|
| `x ∈ Int` | `(declare-const x Int)` | Membership becomes constant declaration |
| `x ∈ Nat` | `(declare-const x Int)` + `(assert (>= x 0))` | Nat is Int with a non-negativity assertion |
| `x ∈ Pos` | `(declare-const x Int)` + `(assert (> x 0))` | Pos is Int with positivity |
| `x ∈ Bool` | `(declare-const x Bool)` | |
| `x ∈ Real` | `(declare-const x Real)` | |
| `x ∈ String` | `(declare-const x String)` | SMT-LIB string theory |
| `x = 5` | `(assert (= x 5))` | |
| `x ≥ 5` | `(assert (>= x 5))` | |
| `a ∧ b` | `(and a b)` | Word-keyword form (`and`, `or`, `not`) is identical |
| `¬a` | `(not a)` | |
| `a ⇒ b` | `(=> a b)` | |
| `∀ i ∈ {0..n} : P` | `(forall ((i Int)) (=> (and (>= i 0) (<= i n)) P))` | Range becomes guard |
| `∃ i ∈ S : P` | `(exists ((i T)) (and (member i S) P))` | Membership keeps its set-theoretic shape |
| `type T = A \| B` | `(declare-datatype T ((A) (B)))` | Algebraic types map directly |
| `type Hero (pos ∈ IVec2)` | `(declare-datatype Hero ((Hero (pos IVec2))))` | Records are single-constructor datatypes |

### What stays Evident-only

These have no place in SMT-LIB; they're program-organization features that compile away before constraint translation:

- **Trace blocks** — testing harness, not constraints
- **Shader blocks** — GLSL transpilation target, not constraints
- **Claim composition** (`..PlatformerMain`, names-match) — sugar that inlines into a flat constraint set
- **Subclaims** — local naming, no semantic content
- **Plugin directives** — runtime concerns

### What SMT-LIB has that Evident doesn't (yet)

For an SMT-LIB → Evident importer, these gaps would need handling:

- **Bitvectors** (`(_ BitVec 32)`) — fixed-width integers. Not in Evident's surface.
- **Arrays** (`(Array Int Bool)`) — function-shaped maps. Evident has `Seq` (positional) which doesn't quite fit.
- **Uninterpreted sorts** — abstract types without structure. Evident always wants concrete types.
- **Quantifier patterns** — solver hints. Evident hides these.
- **Recursive function definitions** (`define-fun-rec`) — Evident has no user functions yet.

For now: handle the subset that maps cleanly, error loudly on the rest, document the gaps.

---

## The Two-Layer IR

SMT-LIB is the HIR — declarative, constraint-shaped, supports `validate` / `sample` / `execute`-with-search. It's what ships when consumers need full Evident semantics.

The **LIR (function-set IR)** sits *above* SMT-LIB as a derived form, NOT an alternative. When the constraint system reduces (via partial evaluation, bidirectional isolation, etc.) to a closed-form mapping from inputs to outputs, the LIR is just that function. The shader transpiler already does this in-memory; what's new is exposing it as a separate output target.

```
Evident source
  → AST
  → HIR (SMT-LIB)   ← canonical form for validate/sample/general execute
       ↓
  → LIR (function set, derived) ← optimized form for execute-when-deterministic
       ↓
  → backend-specific code (GLSL, Rust, Wasm, …)
```

LIR is an *optimization*, not a different language. If LIR generation succeeds, you get faster execute (no solver needed at runtime). If it fails, you fall back to HIR + solver.

---

## What This Document Implements

This is a deliberately small first step. The goal is to prove the framing works without committing to the full vision.

### In scope (v1)

1. **Evident → SMT-LIB exporter** for the constraint subset:
   - Memberships of primitive types (`Int`, `Nat`, `Pos`, `Bool`, `Real`, `String`)
   - Constraints over those: comparisons, arithmetic, logical ops, equality
   - Quantifiers (`∀`, `∃`) over integer ranges
   - Top-level `claim` decls (each becomes a block of declarations + assertions)
2. **SMT-LIB → Evident importer** for the same subset:
   - `(declare-const x T)` / `(declare-fun x () T)` → `x ∈ T`
   - `(assert e)` → `e`
   - Logical / arithmetic / comparison ops
3. **CLI commands**:
   - `evident export-smt2 <file> <claim>` — dump one claim as SMT-LIB to stdout
   - `evident import-smt2 <file>` — read SMT-LIB, emit Evident source to stdout
4. **Tests**:
   - Roundtrip: small Evident programs export and re-import, second-pass Z3 check matches first
   - Output validation: the SMT-LIB we produce is valid (parses, Z3 accepts it)

### Out of scope (later)

- Datatypes (sum types, records). Need careful mapping for the recursive-Datatype Z3 representation.
- `Seq` and `Set` types. SMT-LIB has `Array` and the `seq` theory; mapping isn't direct.
- Composite types (sub-records like `state ∈ GameState`). Would need flattening.
- Trace / shader / passthrough — explicitly excluded; these don't map to constraints.
- Bitvectors, uninterpreted sorts, recursive functions on the import side.

### Future direction

- LIR generation: when partial-eval succeeds, emit the function set as a separate output.
- Multi-solver backend: pipe the SMT-LIB to CVC5, Yices, etc. and compare results.
- SMT-COMP benchmark import for stress-testing.
- A `--format` flag on `evident query` to send constraints out as SMT-LIB instead of running Z3 in-process.

---

## Implementation Notes

### Output format

SMT-LIB v2 text. Pretty-printed but not aggressively — one s-expression per line for declarations and assertions, indented for readability. Z3 doesn't care about whitespace; humans reading the output do.

```smt2
; Generated by Evident — claim: example
(declare-const x Int)
(declare-const y Int)
(assert (>= x 0))
(assert (= (+ x y) 10))
(check-sat)
(get-model)
```

The leading comment names the source claim so output is debuggable.

### Importer parser

S-expression parser, ~150 lines. Tokens are `(`, `)`, atoms (numbers, strings, symbols). The parsed form is a tree of `Atom(...)` and `List(...)` nodes. Translation to Evident AST is a recursive walk.

The parser is intentionally permissive: ignores comments (`;` to EOL), whitespace, and unrecognized commands (`set-option`, `set-info`, `check-sat`, `exit` — all silently dropped on import since they're solver directives, not constraints).

### Identifier mapping

SMT-LIB allows `|` in symbol names and other characters Evident's identifier rules forbid. On import, sanitize via a deterministic mapping (replace illegal chars with `_`). On export, Evident identifier rules are a strict subset of SMT-LIB's, so no sanitization needed.

### Error handling

- Export: encounter an unsupported construct → error with the construct's source location and a pointer to this design doc.
- Import: encounter an unsupported SMT-LIB feature (bitvectors, arrays, etc.) → error with the offending s-expression and the feature name.
- Roundtrip-broken (export + re-import gives different results): doesn't happen for in-scope features. If it does, it's a bug.

---

## Tests

Three test categories:

1. **Unit tests** for the AST → SMT-LIB translator: small fixtures, exact-string compare on the output.
2. **Unit tests** for the SMT-LIB → AST parser: small SMT-LIB fixtures, check the resulting Evident program parses and queries the same way.
3. **Roundtrip tests**: take an Evident program, export, re-import, run both through Z3, compare SAT/UNSAT and bindings.

Roundtrip is the headline test — it validates both directions in one shot and catches asymmetries.
