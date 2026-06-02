# Completion roadmap: from current state to `runtime/src/` deleted

## Goal

A self-hosted Evident compiler running on the kernel. Once it exists
and passes acceptance tests, `runtime/src/` becomes bootstrap-only
and gets moved out of the main path.

## Scale check

| Component | Current | Final |
|---|---:|---:|
| `runtime/src/` (Rust compiler) | ~10,400 LOC | 0 (moved to `bootstrap/` for new-platform builds only) |
| `kernel/src/` | ~880 LOC | ~900 LOC (stable; minor growth for protocol completeness) |
| `stdlib/` | ~480 LOC | ~3,500-5,500 LOC (the Evident compiler lives here) |

The work is essentially: **migrate ~10,400 LOC of Rust into ~4,000
LOC of Evident**. Each Rust file in `runtime/src/` becomes one or
more Evident files in `stdlib/`. Per-stage LOC tends to be lower in
Evident because we don't need types/lifetimes plumbing.

## Phases

Phases are sequential — each depends on the previous. Within a
phase, sub-steps can be parallelized across sessions.

### Phase A — Lexer parity (~stdlib/lexer.ev: 104 → ~400 LOC)

**Goal**: `stdlib/lexer.ev` produces the same TokenList for any
input that `runtime/src/lexer.rs` does. Run as a multi-tick FSM on
the kernel.

| Sub-step | Adds | Acceptance |
|---|---|---|
| A1 | Unicode operators: ∈, ⇒, ⟨, ⟩, ↦, ≤, ≥, ≠, ∧, ∨, ¬, ∀, ∃, →, ⟸ | One test per operator → correct Token variant |
| A2 | Two-char ASCII ops: `==`, `::`, etc. | Lexer recognises both single and pair forms |
| A3 | String literals `"…"` with `\n`/`\t`/`\"`/`\\` escapes | Mode-state pattern (iter 3.7) extended for in-string mode |
| A4 | Float literals (`3.14`, `1e9`) | Decimal-point in the digit accumulator |
| A5 | Full keyword set (all `Keyword` variants from `runtime/src/core/ast.rs`) | MaybeKeyword table covers all keywords |
| A6 | Indentation tracking | Emits INDENT/DEDENT tokens at start of relevant lines |
| A7 | CRLF normalization, EOF handling | Equivalent to lexer.rs end-handling |

**Phase A complete when**: an oracle harness runs every
`tests/lang_tests/*.ev` through both the Rust lexer and the Evident
lexer (running on the kernel), and TokenLists match byte-for-byte.

**Estimated**: 5-10 sessions.

### Phase B — Parser parity (~stdlib/parser.ev: 57 → ~800 LOC)

**Goal**: `stdlib/parser.ev` produces the same AST for any TokenList
that `runtime/src/parser/` does.

| Sub-step | Adds | Acceptance |
|---|---|---|
| B1 | Multi-binop precedence (`*` > `+` > `=`) | `1 + 2 * 3` parses to `EBinOp(+, 1, EBinOp(*, 2, 3))` |
| B2 | Parenthesized subexpressions | `(1 + 2) * 3` correct |
| B3 | Type parsing (`Int`, `Seq(T)`, generics `Edge<T>`) | Type-position FSM mode |
| B4 | Membership body items (`x ∈ Type`, chained-membership) | `BIMembership(name, type, pins)` |
| B5 | Schema declarations (`claim Name body…`) | `MakeSchemaDecl(keyword, name, count, body)` |
| B6 | Enum declarations | `MakeEnumDecl(name, variants)` |
| B7 | Pull-up `..ClaimName`, names-match composition | `BIPassthrough(name)` + `ClaimCall` |
| B8 | Quantifiers `∀ x ∈ S : body` | `EForall(vars, range, body)` |
| B9 | Match expressions + patterns | `EMatch(scrutinee, arms)` |
| B10 | All 7 composition mechanisms surfaced as parse productions | Coverage check vs CLAUDE.md spec |
| B11 | Import statements | `Imports` carried in `Program` |

**Phase B complete when**: an oracle harness runs every
`tests/lang_tests/*.ev` through both the Rust parser and the Evident
parser, ASTs match byte-for-byte (modulo lossless structural diffs).

**Estimated**: 10-20 sessions. The recursive-descent shape becomes
many mode-state FSMs; precedence becomes shift-reduce inside the
Expr mode.

### Phase C — AST → SMT-LIB translator (~stdlib/translate.ev: 0 → ~2,000 LOC)

**The biggest phase.** `runtime/src/translate/` is ~6,000 LOC of
Rust; mapping to Evident yields roughly 1,500–2,500 LOC.

| Sub-step | Adds | Acceptance |
|---|---|---|
| C1 | Z3 datatype declarations: `Seq(T)`, enums (incl. `__SeqOf_*` cells) | Datatype block matches `runtime/src/runtime/register_enums.rs` output |
| C2 | Per-primitive variable declarations (`declare-fun`) | Matches `runtime/src/translate/declare.rs` |
| C3 | Boolean expression translation (∧, ∨, ¬, ⇒, =, ≠, <, ≤, ∈) | One sub-claim per operator |
| C4 | Arithmetic translation (+, -, *, /, #seq, …) | Matches `translate/exprs/scalar.rs` |
| C5 | Claim composition (the 7 mechanisms inlined into body) | Matches `translate/inline/*` |
| C6 | Quantifier expansion (`∀ i ∈ {0..n-1}`) | Pinned-range unroll |
| C7 | Seq operations (literal, equality, index, length) | Matches `translate/exprs/seq_eq.rs` |
| C8 | Match → nested ITE | Matches `translate/exprs/match_expr.rs` |
| C9 | Record lift (componentwise `=`, arithmetic broadcast, `IVec2(...)` literal) | Matches `translate/exprs/record_lift.rs` |
| C10 | String operations | Matches `translate/exprs/string_ops.rs` |
| C11 | Generic monomorphization | Matches `runtime/src/runtime/generics.rs` |
| C12 | Type-inference passes (claim-arg, lhs-eq) | Matches `runtime/src/runtime/inject.rs` (already only 283 LOC Rust) |
| C13 | `++` Seq concat flattening | Matches `runtime/src/runtime/desugar.rs` |
| C14 | Manifest header emission per `docs/plans/kernel-input-spec.md` | Output runnable by the kernel as-is |

**Phase C complete when**: an oracle harness compiles every
`tests/kernel/*.ev` through both `evident emit` (Rust) and the
Evident translator, and the resulting SMT-LIB is semantically
equivalent (kernel produces same output when run on either).

**Estimated**: 15-30 sessions. This is where the LOC budget is
spent. Several sub-steps (C1, C2, C5) are mechanical; others
(C8, C9, C11) need design care.

### Phase D — Pipeline integration

| Sub-step | Adds | Acceptance |
|---|---|---|
| D1 | One Evident program that lexes + parses (no translate yet) | Reads `.ev` file → emits TokenList → emits AST diagnostic |
| D2 | Full pipeline: lex + parse + translate + write `.smt2` | End-to-end smoke test |
| D3 | CLI wrapper: `evident-self emit <file.ev> <claim> -o <out.smt2>` | Drop-in replacement for `evident emit` |

**Phase D complete when**: `evident-self emit hello.ev hello | kernel /dev/stdin`
produces "hello world" exit 0 without any Rust runtime involvement.

**Estimated**: 3-5 sessions. The "compose multiple FSMs into one
program" pattern is the main risk; the iter 3.14 mega-pipeline
experiment showed the parser tolerance issue. Mitigation: write each
stage as a separate `claim` and have a top-level `main` that calls
them via state transitions (mode 0 → 1 → 2 → 3 → 4).

### Phase E — Bootstrap acceptance

| Sub-step | Adds | Acceptance |
|---|---|---|
| E1 | Self-compile: feed the Evident compiler its own source | Output `.smt2` runs on the kernel and produces a compiler too |
| E2 | Diff-test: byte-for-byte (or behaviour-for-behaviour) match between Rust and self-hosted output on a corpus | All 175 lang test claims + all 24 kernel tests pass through self-hosted compiler |
| E3 | Performance: self-hosted compile completes in reasonable time on the kernel | Compiling a 100-line `.ev` file → `.smt2` finishes in < 60s |

**Phase E complete when**: a `./test.sh --self-hosted` flag runs the
full test suite through the Evident compiler and gets the same
results as the Rust compiler.

**Estimated**: 3-10 sessions, mostly diagnosing equivalence failures.

### Phase F — Replace and delete

| Sub-step | Action |
|---|---|
| F1 | `evident` CLI defaults to the self-hosted compiler |
| F2 | Move `runtime/src/` → `bootstrap/runtime/src/`, document as "used once per new platform"  |
| F3 | Remove Rust compiler from the main build (`evident` binary is now a thin shell over kernel + bootstrap .smt2) |
| F4 | Rewrite `CLAUDE.md` to reflect: kernel + stdlib + self-hosted compiler IS the project; runtime/ is archeology |
| F5 | Update `./test.sh` to skip the Rust phases; cargo only builds the kernel |

**Phase F complete when**: `runtime/src/` is no longer in the main
path. `evident` works end-to-end via kernel + stdlib only.

**Estimated**: 1-2 sessions of plumbing + 1 session of CLAUDE.md
rewrite.

## Total scope estimate

37-77 sessions across all phases. Wide range because Phase C is
hard to estimate without picking implementation patterns. Realistic
midpoint: ~55 sessions of focused work.

## Dependencies between phases

```
A (lexer)  ──→  B (parser)  ──→  C (translator)  ──→  D (pipeline)  ──→  E (bootstrap)  ──→  F (delete)
```

Sub-steps within a phase are often parallel-able (A1 and A3 are
independent). But A must complete before B starts (parser consumes
tokens that the lexer hasn't produced for yet); B before C; etc.

## What blocks the user from starting Phase F today?

Almost everything in A, B, C is missing. The toy versions
demonstrate the architecture; they don't approach feature parity.

The honest read: Phase A alone needs ~5-10 sessions before Phase B
can start; Phase B needs ~10-20 before C; C is the biggest. Total
time-to-completion is dominated by C.

## Acceptance criteria summary

The project is "complete" when ALL of the following are true:

1. `evident sample <file> <claim>` works without touching
   `runtime/src/`, by going through the self-hosted compiler.
2. `evident emit <file> <claim>` likewise.
3. `evident run <file> <claim>` likewise.
4. All 175 lang test claims pass when their compilation goes through
   the self-hosted compiler.
5. All 24 kernel tests pass when their `emit` goes through the
   self-hosted compiler.
6. `runtime/src/` is not on the `main` build path (moved to
   `bootstrap/` or deleted outright).
7. `CLAUDE.md` is rewritten with the new floor as load-bearing
   reality, not aspiration.

When all seven hold, the project as currently scoped is done.

## How to work on this plan

- **Session prep**: read CLAUDE.md (invariants) + this roadmap +
  any relevant phase status (none yet — to be created per phase as
  they progress).
- **Session execution**: pick one sub-step, complete it end-to-end
  (stdlib code + test fixture + commit + push).
- **Session handoff**: append progress to the phase's status doc (or
  update this roadmap's acceptance state) before stopping.
- **Per-iteration commit message convention**:
  `<phase>.<substep>: <one-line description>` — e.g.
  `A1: unicode operators ∈ ⇒ ⟨ ⟩ ↦ in lexer`
