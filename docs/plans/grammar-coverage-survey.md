# Grammar coverage survey

Read-only survey (task #26). No code changed. Goal: enumerate every
Evident grammar shape actually USED across the source corpora, map it
against what `compiler/compiler.ev` handles today, and propose the wave
order that reaches self-compilation with no wasted steps.

**Corpora scanned** (occurrence counts are `rg -o` aggregates per tree):

| tree              | files | role                                   |
| ----------------- | ----- | -------------------------------------- |
| `compiler/*.ev`   | 17    | the self-hosted compiler's own source  |
| `stdlib/*.ev`     | 3     | library the compiler/programs depend on (`kernel.ev`, `combinatorics.ev`, `toposort.ev`) |
| `tests/kernel/*.ev` | 74  | kernel-runnable integration fixtures   |
| `tests/lang_tests/*.ev` | 11 | language-spec corpus                  |

Counts are approximate (regex over Unicode source, comments included in
a few rows); treat them as "heavily / lightly / not used," not exact.

---

## Section 1 — Universe of grammar shapes used

Columns: occurrences in **comp** / **std** / **kern** / **lang**.

### Declarations

| shape                                  | comp | std | kern | lang |
| -------------------------------------- | ---- | --- | ---- | ---- |
| `claim` decl                           | 132  | 49  | 148  | 306  |
| `type` decl (record/nominal)           | 0    | 2   | 2    | 4    |
| `schema` decl                          | 0    | 0   | 0    | 0    |
| `fsm` decl                             | 0    | 0   | 0    | 0    |
| `enum` decl                            | 44   | 6   | 0\*  | 49   |
| `enum` variant w/ payload `V(T,…)`     | 222  | 11  | 296  | 16   |
| `import "…"`                           | 33   | 2   | 143  | 1    |
| `subclaim Name` (nested registered)    | 2    | 0   | 9    | 1    |

\* `tests/kernel` enums are declared via imports (stdlib/compiler) and
through `enum` lines inside imported fixtures; the 0 is the literal
`enum` keyword at top of those files — enum *usage* is heavy (variant
payload row).

`schema` and `fsm` keywords: **zero occurrences anywhere.** `claim`
and `type` are the only schema keywords in live use. (`fsm`-as-keyword
matters per memory `[[feedback_fsm_keyword_is_sole_signal]]` but no
corpus file uses it yet.)

### Membership & chained membership

| shape                                  | comp | std | kern | lang |
| -------------------------------------- | ---- | --- | ---- | ---- |
| `x ∈ T` (declare / iter / type-use)    | 620  | 70  | 1453 | 223  |
| `x ∈ T = rhs` (declare + pin)          | heavy| heavy| heavy| heavy|
| chained `0 < x ∈ Int < 10`, multi-name | rare | rare | some | some |

Membership is the single most common shape. The MVP driver handles
exactly one membership per program; the corpus averages dozens.

### Expression atoms

| shape           | comp | std | kern | lang |
| --------------- | ---- | --- | ---- | ---- |
| `EInt` literal  | 132  | 55  | 981  | 192  |
| `EIdent`        | ubiq | ubiq| ubiq | ubiq |
| `String` literal `"…"` | 1201 | 48 | 4219 | 78 |

### Binary / unary operators

| op           | comp | std | kern | lang |
| ------------ | ---- | --- | ---- | ---- |
| `+`          | 29   | 3   | 149  | 15   |
| `-`          | 6    | 0   | 66   | 3    |
| `*`          | 28   | 6   | 54   | 3    |
| `/`          | (≈, comment-noisy) | – | – | – |
| `=` (eq/pin) | 614  | 60  | 2020 | 301  |
| `≠`          | 4    | 0   | 2    | 19   |
| `<`          | 0    | 5   | 14   | 19   |
| `≤`          | 8    | 1   | 20   | 8    |
| `>`          | 4    | 0   | 25   | 5    |
| `≥`          | 8    | 0   | 63   | 0    |
| `∧`          | 65   | 0   | 268  | 5    |
| `∨`          | 138  | 0   | 64   | 2    |
| `⇒`          | 240  | 0   | 415  | 29   |
| `¬`          | 14   | 0   | 120  | 0    |

### Control / value forms

| shape                              | comp | std | kern | lang |
| ---------------------------------- | ---- | --- | ---- | ---- |
| ternary `c ? a : b`                | 191  | 0   | 732  | 15   |
| `match` expr                       | 149  | 0   | 220  | 14   |
| match arm `pat ⇒ expr`             | 239  | 0   | 410  | 29   |
| match patterns: `Ctor(binds)`, `_` | heavy| –   | heavy| some |
| match **guards**                   | not observed (no `when`/guarded-arm form in corpus) | | | |
| `e matches Ctor(_)` recognizer     | 132† | 4   | 77   | 20   |

† compiler count inflated by the word "matches" in prose comments; the
*construct* is used in parser/lexer recognizers.

### Seq / String ops

| shape                        | comp | std | kern | lang |
| ---------------------------- | ---- | --- | ---- | ---- |
| `Seq(T)` type                | 9    | 24  | 102  | 7    |
| seq literal `⟨…⟩`            | 16   | 13  | 195  | 8    |
| concat `++`                  | 59   | 0   | 130  | 0    |
| cardinality / length `#x`    | 22   | 21  | 60   | 3    |
| index `seq[i]`               | 4    | 14  | 35   | 3    |
| `substr`                     | 3    | 0   | 28   | 0    |
| `coindexed` / `edges`        | 1    | 0   | 0    | 0    |

### Quantifiers

| shape          | comp | std | kern | lang |
| -------------- | ---- | --- | ---- | ---- |
| `∀ x ∈ S : …`  | 8\*  | 7   | 4    | 0    |
| `∃ …`          | 4\*  | 0   | 3    | 0    |

\* in `compiler/` ∀/∃ appear only in **comments + lexer/parser
recognizers** (`OpForall`, `EForall`). The compiler driver does not
*use* a quantifier in its own logic. Real usage is in `stdlib`
(toposort/combinatorics) and a few kernel fixtures. Only the
single-name shape `∀ x ∈ S : body` is used; the tuple-binding form
`∀ (a,b) ∈ coindexed(…)` is documented-but-unused in the compiler
(parser.ev:39-40 notes it as out of scope).

### Composition mechanisms (CLAUDE.md §"Composition mechanisms")

| form                          | comp | std | kern | lang |
| ----------------------------- | ---- | --- | ---- | ---- |
| `ClaimName(slot ↦ value)`     | 157  | 0   | 443  | 11   |
| `..ClaimName` inline          | 14   | 3   | 19   | 2    |
| bare `ClaimName` (names-match) | present (hard to count distinctly) | | | |
| `(a,b) ∈ ClaimName` positional | rare | rare | some | some |
| `cond ⇒ ClaimName` conditional | via `⇒` rows above | | | |
| `recv.subclaim(args)`         | 1    | 1   | 0    | 0    |
| `subclaim Name` nested        | 2    | 0   | 9    | 1    |

Slot-bind composition (`↦`) is the dominant composition form and is
used **pervasively in `compiler.ev` itself** (every `IsAlphaChar(c ↦
…)`, `ParseMembership(head1 ↦ …)`, `DeclareFromMembership(item ↦ …)`).

### Generics

| shape                  | comp | std | kern | lang |
| ---------------------- | ---- | --- | ---- | ---- |
| `<T>` instantiation    | 0\*  | 43  | 19   | 0    |

\* the one `<…>` in `compiler/parser.ev` is a comment example. Generics
are used by `stdlib/toposort.ev`, `stdlib/combinatorics.ev`, and
`tests/kernel/{test_parser_types,test_pipeline_full_d2,test_string_lit,
test_translate_generics}.ev` — **not** by the compiler driver's own
import chain.

### Effects / kernel runtime conventions

| shape                       | comp | std | kern | lang |
| --------------------------- | ---- | --- | ---- | ---- |
| `is_first_tick`             | 6    | 0   | 193  | 0    |
| `_<name>` state-carry       | 24   | 0   | 597  | 1    |
| `last_results[…]` match     | 2    | 2   | 24   | 1    |
| `effects = …`               | 11   | 3   | 104  | 2    |
| `LibCall(…)`                | 2    | 10  | 164  | 4    |
| `Exit(n)`                   | 1    | 4   | 84   | 5    |
| `ReadFile`                  | 2    | 5   | 16   | 0    |
| `WriteFile` / `ReadLine`    | 0    | 5   | 2/3  | 0    |
| `Build*` sugar              | 0    | 6   | 8    | 2    |
| `Result` match `Ok/Err`     | 1    | 0   | 1    | 29   |
| FTI (`Stack`/`Queue`)       | 1    | 0   | 4    | 0    |

`compiler.ev` already uses: `is_first_tick`, `_<name>` carry,
`last_results[0]` match, `effects = (… ? … : …)`, `LibCall`/`Exit`,
`ReadFile`. These are self-hosting essentials.

**Distinct grammar shapes catalogued: ~40** (counting each row above as
one shape family).

---

## Section 2 — Coverage map

### What `compiler/compiler.ev` handles TODAY (the MVP)

The driver (`compiler/compiler.ev`) reads one `.ev` via `ReadFile`,
runs the consolidated-lexer FSM, peels a fixed 5-token reverse
`TokenList`, and emits a manifest + one `declare-fun` + one `assert`.
Concretely it covers exactly:

- **one** schema head `<kw> Ident`
- **one** primitive membership `Ident ∈ Type` with a single `= N`
  ASCII-integer pin
- declare emission (`translate_declare`), `EBinOp(OpEq,…)` →
  `(= …)` (`translate_bool`), 5-line manifest (`translate_manifest`)

Wired passes: declare + bool-eq + manifest **only**. The other 12
`translate_*.ev` files exist as **per-pass fixtures, not composed into
the driver** (`compiler/README.md`: "no real driver yet; per-pass
fixtures only"). The *parser* recognises far more than the *driver*
consumes — see Section 4.

### What wave 1 adds (session A, in flight)

- Multiple body items (membership list, not just one).
- Arithmetic expression pins (`= a + b * c`, not just `= N`).

### Per-shape disposition

Legend: **W2** = wave-2 candidate (small next step) · **W3+** = bigger
lift · **BLOCKER** = used by `compiler.ev`'s own import chain, so
self-compilation is impossible without it · **STDLIB/TEST-ONLY** = used
in corpus but not on the compiler-driver self-host path.

| shape | disposition | note |
| ----- | ----------- | ---- |
| `claim`/`type` decl | done (head) → **W2** body | multi-item body is wave 1/2 |
| `schema`/`fsm` decl | not used | skip until a corpus file needs it |
| multi membership in one body | **W2** (wave 1) | core widening |
| `x ∈ T = N` integer pin | **DONE (MVP)** | |
| arithmetic pin `+ - *` | **W2** (wave 1) | `translate_arith` exists, needs wiring |
| `=` / `≠` eq pin | bool eq **DONE**; `≠` **W2** | `translate_bool` |
| comparisons `< ≤ > ≥` | **W2** | `translate_bool` covers shape; wire it |
| `∧ ∨ ¬ ⇒` boolean | **BLOCKER** + **W2** | `compiler.ev` uses ⇒/∧/∨/¬ everywhere; needed to self-compile; `translate_bool` is the pass |
| ternary `? :` | **BLOCKER** + **W2/W3** | `compiler.ev` is built almost entirely on nested ternary (191 uses); **must** translate ternary → ITE to self-compile. No dedicated `translate_ternary` exists yet — gap. |
| `match` + ctor patterns + binds | **BLOCKER** + **W3** | `compiler.ev` uses match on `last_results`, `_tokens`, `mem_item`; `translate_match` exists as fixture, must compose |
| `_` wildcard pattern | **BLOCKER** + **W3** | with match |
| match guards | not used | skip |
| `e matches Ctor(_)` | **W3** | used in parser/lexer; lower to a tester |
| `enum` decl + payload variants | **BLOCKER** + **W3** | `compiler.ev` + its imports define many enums (`Token`, `Expr`, `BodyItem`, …); `translate.ev` (datatypes) is the pass, must compose |
| `Seq(T)` + `⟨…⟩` literal | **BLOCKER** + **W3** | effects is `Seq(Effect)`; `translate_seq` |
| concat `++` | **BLOCKER** + **W3** | `compiler.ev` builds output via `++`; `translate_concat` |
| cardinality `#x` | **BLOCKER** + **W3** | `compiler.ev` uses `#input`, `#_partial_str` |
| index `seq[i]` | **W3** | stdlib/tests; `last_results[0]` is index-on-Seq |
| `substr` / string ops | **BLOCKER** + **W3** | `compiler.ev` lexer uses `substr`; `translate_string` |
| String literal | **BLOCKER** + **W2** | output is built from string literals; ASCII string literal emission |
| `∀ x ∈ S :` / `∃` | **STDLIB/TEST-ONLY** + **W4** | not used by driver; `translate_quant` |
| `(a,b) ∈ …` tuple ∀ | not used in compiler | **W4+**, parser already defers it |
| `ClaimName(slot ↦ value)` | **BLOCKER** + **W3** | `compiler.ev` composes every pass via slot-bind; `translate_compose` |
| `..ClaimName` / bare names-match | **BLOCKER** + **W3** | `translate_compose` |
| `(a,b) ∈ ClaimName` positional | **W4** | corpus-light |
| `cond ⇒ ClaimName` conditional | **W3** (with ⇒ + compose) | |
| `recv.subclaim(args)` | **W4** | rare |
| `subclaim Name` nested | **W4** | rare; appears in compiler/kernel a little |
| generics `<T>` | **STDLIB/TEST-ONLY** + **W4** | not on driver path; `translate_generics` |
| `is_first_tick` | **BLOCKER** + **W2** | driver uses it; emit auto-injects |
| `_<name>` state-carry | **BLOCKER** + **W2** | driver carries `_input`,`_pos`,`_tokens`,… |
| `last_results[i]` match | **BLOCKER** + **W3** | driver reads file result this way (Seq index + match) |
| `effects = … (ternary)` | **BLOCKER** + **W2/W3** | driver emits effects via ternary over Seq literals |
| `LibCall` / `Exit` / `ReadFile` | **BLOCKER** + **W3** | driver's I/O; these are enum payload + Seq literal shapes |
| `WriteFile`/`ReadLine` | **W4** | not on driver path |
| `Build*` sugar | **W4** (or transitively via compose) | stdlib sugar over LibCall |
| `Result` Ok/Err match | **W4** | mostly lang_tests; a match special case |
| FTI Stack/Queue | **W4+** | advanced; `[[project_fti_honesty_audit_result]]` |
| `import` | **BLOCKER** + **W2/W3** | driver imports 6 files; the compiler must resolve imports to self-compile (or be pre-concatenated — see Open Q) |

---

## Section 3 — Recommended wave plan

Ordering principle: each wave only depends on earlier deliverables; the
target is the **minimum set that lets `compiler.ev` compile itself**
(every **BLOCKER** row above). The blockers, grouped by difficulty,
*are* the wave plan. Stdlib/test-only shapes come after self-host is
reached.

### Wave 1 (in flight, session A) — multi-membership + arithmetic
- multiple body items per schema (membership list)
- arithmetic expression pins (`+ - *`) via `translate_arith`

### Wave 2 — scalar bodies & flow primitives (the cheap blockers)
Goal: a body of N memberships, each pinned by a scalar/boolean
expression, with tick conventions.
- comparisons `< ≤ > ≥` and `≠` (extend `translate_bool`)
- boolean connectives `∧ ∨ ¬ ⇒` (extend `translate_bool`) — **blocker**
- **ternary `? :` → ITE** — new pass (no `translate_ternary` exists);
  highest-value single item, `compiler.ev` is built on it — **blocker**
- ASCII `String` literal emission — **blocker**
- `is_first_tick` auto-inject + `_<name>` state-carry recognition —
  **blocker** (mostly emit/manifest plumbing)

Deliverable: can compile a multi-membership scalar/bool/ternary FSM
body. Does **not** yet handle enums/match/Seq/compose.

### Wave 3 — the self-hosting core (the structural blockers)
Goal: everything `compiler.ev`'s own source needs that wave 2 lacks.
This is the wave that unlocks self-compilation.
- `enum` decl + payload variants → SMT-LIB datatypes (compose
  `translate.ev`) — **blocker**
- `match` over constructor patterns + binds + `_` → nested ITE over
  testers (compose `translate_match.ev`) — **blocker**
- `e matches Ctor(_)` recognizer (tester lowering)
- `Seq(T)` + `⟨…⟩` literal + `++` concat + `#` cardinality +
  `seq[i]`/`last_results[i]` (compose `translate_seq`, `translate_concat`)
  — **blocker**
- `substr` + string ops (compose `translate_string`) — **blocker**
- claim composition: `ClaimName(slot ↦ value)`, `..ClaimName`, bare
  names-match, `cond ⇒ ClaimName` (compose `translate_compose`) —
  **blocker**
- `effects = … (ternary over Seq literals)`, `LibCall`/`Exit`/`ReadFile`
  as enum-payload + Seq shapes — **blocker**
- `import` resolution (or decide on pre-concatenation — Open Q #1) —
  **blocker**

Deliverable: `compiler.ev` compiles itself via `kernel +
compiler.smt2` → byte/semantic-equivalent to the bootstrap-produced
`compiler.smt2`. **This is the self-compilation milestone** (Phase 3/4
of DELETION-CHECKLIST).

### Wave 4 — corpus completeness (stdlib + test-only shapes)
Goal: compile the *rest* of the corpus, not just the compiler.
- quantifiers `∀ x ∈ S :` / `∃` (compose `translate_quant`)
- generics `<T>` monomorphization (compose `translate_generics`,
  `translate_infer`) — see `[[project_generics_selfhost_result]]`
- record `type T(a,b ∈ …)` lifts (compose `translate_record`)
- `(a,b) ∈ ClaimName` positional, `recv.subclaim`, `subclaim Name`
- `Result` Ok/Err idioms, `Build*` sugar, `WriteFile`/`ReadLine`
- tuple-binding `∀ (a,b) ∈ coindexed(…)`

### Wave 5+ — advanced / perf
- FTI (`Stack`/`Queue`) self-host (`[[project_fti_honesty_audit_result]]`)
- chained membership `0 < x ∈ Int < 10`, multi-name decls
- any shape surfaced by running the full conformance corpus under
  `IMPL=selfhost`

The first three waves (1–3) are the critical path to deleting
bootstrap; wave 4 reaches full corpus equivalence (`IMPL=both`).

---

## Section 4 — Open questions

1. **Imports: resolve or pre-concatenate?** `compiler.ev` imports 6
   files. To self-compile it, the self-hosted compiler must either
   implement `import` resolution (read + splice) or the build must
   pre-concatenate sources before handing one buffer to the compiler.
   The parser has `ImportPath`/`BadImport` AST but the driver doesn't
   resolve them. **Decide before wave 3** — it changes whether
   `import` is a compiler feature or a build-script concern.

2. **Parser supports more than the translator wires.** `parser.ev`
   produces `BIPassthrough`, `BIConstraint`, `BIClaimCall`,
   `BISubclaim`, `EForall`, `EExists`, `EMatch`, enum/import ASTs — but
   the driver only consumes a 5-token membership-with-pin slice and no
   `translate_*` pass is composed for most of these. The parser is
   ahead of both the driver and several translators. Which is the
   binding constraint per shape needs a pass-by-pass audit when wiring
   wave 3.

3. **No `translate_ternary.ev` exists.** Ternary is the single most
   used control form in `compiler.ev` (191×) and a hard self-host
   blocker, yet there is no dedicated per-pass file for it (unlike
   match/arith/bool). Is ternary lowered inside `translate_bool`/an
   expr dispatcher, or does it need its own pass? Resolve early in
   wave 2.

4. **Consolidated-lexer vs `lexer.ev` classifiers.** `compiler.ev`
   inlines a hand-written consolidated-lexer FSM (the `partial_str` /
   `partial_int` / `tokens` block) rather than calling `lexer.ev`'s
   token producers directly. When the grammar widens, does the driver
   keep the inline FSM or switch to a composed lexer? Affects how much
   of `lexer.ev` is actually on the self-host path.

5. **`match` exhaustiveness / fallthrough.** Corpus uses `_`
   wildcards heavily but no match *guards*. Confirm the translator
   target only needs constructor-tester ITE chains with a default arm
   (no guard lowering) — appears true from the corpus but worth
   asserting before building `translate_match` into the driver.

6. **State-carry typing.** Per CLAUDE.md only primitive
   (`Int/Bool/Real/String`) top-level memberships carry. `compiler.ev`
   carries `_tokens ∈ TokenList` (an enum), which is **not** primitive
   — so it relies on a non-primitive carry. Is enum/Seq state-carry a
   kernel capability the self-hosted emit must reproduce, or does the
   driver special-case it? (cf. `[[project_cons_to_seq_sweep_blocked]]`
   — Seq carry landed but is perf-gated.) Needs confirmation before
   wave 3 effects/match wiring.

---

*Survey only. `./test.sh` untouched; diff is this one file.*
