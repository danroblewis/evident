# Evident purism — what the language wants to be

The canonical adherence reference for the `evident-critic` skill
(`.claude/skills/evident-critic/SKILL.md`). It judges **surface
language as written**, never what the toolchain makes of it.

**Admissibility rule (non-negotiable).** The project compiles via
pre-oracle text transforms (`scripts/passes/`), so it is possible to
invent grammar, teach a transform to lower it, keep every gate green —
and still have written something that is not Evident. Therefore:
"the pipeline accepts it," "the transform handles it," and
"conformance passed" are **inadmissible evidence** of language
adherence. The only admissible evidence is this document's catalog,
CLAUDE.md's spec sections, conformance-pinned grammar, and explicit
operator rulings.

**Out of scope: performance.** Functionization, Z3 solve cost, the
`≠`-disequality cliff, covered-vs-implication-defined outputs — all of
it has its own gates (`scripts/functionization-gate.sh`,
`scripts/perf-profile.sh`) and its own doc (CLAUDE.md's perf caveats).
The critic must **never block or excuse on perf grounds**. Where a
pretty surface form is slow, the verdict is still "the surface is
right" — the fix belongs in the lowering (see §1, "keep the surface").

---

## 1. Philosophy — the language's soul

An Evident program is a **collection of constraints over named
variables**. The central abstraction — `schema` / `type` / `claim` —
is a **named set defined by membership conditions**. A solver finds
satisfying assignments; the program never says *how*. Mathematical
notation (`∀ ∃ ∈ ⇒ ∧ ∨ ¬`, set theory) is a **primary product goal**,
not decoration: the surface should read as mathematics. Declarative
over operational, always.

Operator rulings, binding on all reviews:

1. **"We don't do functions."** There are no function-call-looking
   constructs that return values. Claims **compose**; they are not
   *called for values* — slot mappings (`Sub(slot ↦ value)`) wire
   constraints between named sets. A proposed `find(...)` value-lookup
   form was rejected on exactly this ground; that rejection is
   precedent. Anything shaped like `name(args)` yielding a value in an
   expression position — other than enum constructors, record
   literals/pins, and the blessed builtin forms in §2 — is a BLOCKER.

2. **Finite state machines stay FINITE.** Carried collections are
   statically bounded (`#xs ≤ N` with a literal bound). Unbounded data
   lives on "the tape" — FTI buffers reached through effects — never in
   carried `Seq` or cons-list growth. A carried collection with no
   static bound is not a finite state machine and is a BLOCKER.

3. **A type is its constraints, not its fields.** The header names the
   fields; the **body** is where the type earns its keep, binding the
   fields' relationships (`0 ≤ count ≤ cap`; `sol > 0 ⇒ (ctx > 0 ∧
   cfg > 0)`). A bodyless `type T(a, b)` is an anemic tuple. Lifecycle
   types use **conditional** invariants for the boot window (`live ⇒
   …`). (How the liveness test is *spelled* — `> 0` vs `≠ 0` — is a
   perf matter, out of critic scope.)

4. **Ternary chains are an ite-shaped implementation detail.** The
   operator hates them in source. Set-theoretic surfaces — keyed-
   projection pin pairs, guarded implications — are the preferred form;
   covered select chains belong ONLY in lowered artifacts. The one
   blessed exception is the carried-write hold chain — see §3.4 for
   the full ruling.

5. **Keep the surface, change the lowering.** When a pretty surface
   form is slow or unsupported, the fix is a transform/lowering change —
   NEVER rewriting source into the fast encoding. This is project
   doctrine, stated in `docs/plans/post-cutover-roadmap.md` (resolution
   of 2026-06-09): the source says `evt ∈ Seq(EnumVariantVal)` while
   the oracle sees numbered scalars; "the measured walls are walls of
   the *oracle-visible encoding* … the *surface* is free." Rewriting
   `∀ e ∈ xs` back into `e0/e1/e2` scalars to dodge a lowering gap is a
   purity regression even though it is valid grammar.

6. **The relational reading.** Claims are relations; composition is a
   join; `(slot ↦ value)` is ρ (rename). Joins operate on the declared
   schema, never on incidental column names (the headers-as-interface
   direction, `docs/plans/claim-headers-interface.md`, operator-approved
   2026-06-10). When judging a composition, ask what relation it joins
   and on which names.

---

## 2. The blessed grammar catalog

Everything that IS Evident. A construction not in this catalog, not in
CLAUDE.md's spec sections, not pinned by a conformance fixture, and not
in an operator-approved plan **is not Evident** — even if a transform
lowers it today (§5).

### 2.1 Schema keywords

| Keyword  | Use |
| -------- | --- |
| `type`   | Record / nominal value type (a noun). Header names fields; body states invariants. |
| `claim`  | Predicate / constraint / property (verb-like). Pure: no autocarry; explicit `_x` decls if it reads prev-tick state. |
| `fsm`    | A claim that carries state across ticks; `_x` carry-duals auto-synthesized for referenced prev-tick fields. Use for anything stateful. |
| `schema` | Synonym for `type`. Avoid in new code. |

`sat_*` / `unsat_*` test claims are written `claim`. Using `claim` +
hand-written `_x` decls where `fsm` would synthesize them is
dispreferred; using `fsm` for a pure predicate is wrong.

### 2.2 Composition (current semantics, pinned 2026-06-10 by conformance 139/140/141)

| Form | Meaning |
| ---- | ------- |
| `v ∈ TypeName`            | Typed variable; fields/invariants **receiver-scoped** (`v.field`). Zero implicit sharing. |
| `..ClaimName`             | **LIFT**: inline the body in the caller's scope (shared names, names-match). The deliberate everything-shared form. |
| `ClaimName` (bare)        | **CALL**: parent names pass down; the claim's own unmapped internals are **HIDDEN** (fresh per call site). |
| `ClaimName(slot ↦ value)` | CALL with explicit slot binding; unmapped internals hidden. |
| `(a, b) ∈ ClaimName`      | Positional binding to first-line params. |
| `cond ⇒ ClaimName`        | Conditional inline. |
| `recv.subclaim(args)`     | Subclaim dispatch with receiver prefix. |
| `subclaim Name`           | Nested claim registered top-level. |

Bare mention and `..` are NOT synonyms. Use bare/call for components;
`..` only for deliberate context sharing; receiver instances for zero
implicit sharing. **Approved direction** (claim-headers plan): a
claim/fsm header declares its interface — bare mention joins on header
names only, any mapping is explicit-only (with punning: bare `name` ≡
`name ↦ name`), body memberships never join. Headers in new code are
aligned with the ideal; reliance on whole-body implicit interface in
*new* components is the thing headers exist to retire.

### 2.3 Chained membership

```evident
x ∈ Int = 5            -- declare + pin
x ∈ Int < 10           -- declare + upper bound
0 < x ∈ Int < 10       -- declare + range
a, b, c ∈ Int < 5      -- multi-name
```

### 2.4 Records and lifts

`type IVec2(x, y ∈ Int)` with four automatic lifts: componentwise
comparison (`a < b`, `lo ≤ x ≤ hi`), arithmetic broadcast
(`c = a - b`), type-use pins (`pos ∈ IVec2(380, 280)`,
`pos ∈ IVec2(x ↦ 1)`), record literals in expressions
(`state.pos = IVec2(0, 0)`).

### 2.5 Seq

```evident
items ∈ Seq(Int) = ⟨1, 2, 3⟩        -- literal
xs ∈ Seq(Int) = a ++ b ++ ⟨c⟩        -- concat
#items = 3                           -- cardinality
#xs ≤ 8                              -- the static bound (mandatory on carried Seqs — §1.2)
xs[i]                                -- index
∀ x ∈ items : x > 0                  -- element iteration
∃ e ∈ xs : e.name = key              -- existential
∀ (cur, nxt) ∈ coindexed(a, b) : …   -- parallel zip
∀ (a, b) ∈ edges(seq) : …            -- consecutive pairs
∀ k ∈ {0..N} : …                     -- index range (only where position IS the subject — §3.1)
```

**Tuple-binds are legal ONLY over constructions that genuinely yield
pairs**: `coindexed(a, b)` and `edges(seq)`. A tuple-bind over a plain
Seq — `∀ (k, e) ∈ xs` — asserts that `xs` contains (index, element)
tuples, which is false. That form was invented and transform-lowered
once (commit `d1be22a`), and reverted as invalid grammar (`2b0efb2`).
**That incident is the calibration example** for this whole document:
byte-identical lowering output and green gates did not make it Evident.

The keyed-projection pair (the set-theoretic registry read):

```evident
∀ e ∈ xs : ((e.name = key) ⇒ (out = e.val))
(¬(∃ e ∈ xs : e.name = key)) ⇒ (out = default)
```

The registry write surface (`_e` is the bound element's prev-tick
carry dual — the fsm `_x` convention applied to the element):

```evident
∀ k ∈ {0..5} : xs[k].name = (… (alloc ∧ _cur = k) ? new_nm …)
∀ e ∈ xs : e.val = (… (upd ∧ _e.name = key) ? v : _e.val)
```

Membership note: `x ∈ Set(T)` is real grammar. Direct `x ∈ xs` on a
**Seq** is a silently-dropped gap in the frozen oracle (§4 V2); the
expressible form today is `∃ i ∈ {0..#xs-1} : xs[i] = x`.

### 2.6 Enums, match, matches

```evident
enum Color = Red | Green | Blue
enum Result = Ok(Int) | Err(String)
enum LL = Nil | Cons(Int, LL)        -- recursion, forward refs, mutual recursion

n = match e
    Ok(v)  ⇒ v
    Err(_) ⇒ 0

is_ok = e matches Ok(_)
```

Variant names globally unique. Enum constructors in expression position
(`Exit(0)`, `C2ICons(h, t)`) are constructors, not function calls.

### 2.7 Generics

`type Edge<T>(from, to ∈ T)`; `claim Toposort<T>`; explicit type args
only (`Toposort<Rect>(n ↦ 4, …)`); type-parameter names capitalised.

### 2.8 Effects / kernel floor

The `Effect` enum floor (`ReadLine | ReadFile | WriteFile | LibCall |
Exit`); `Build*` sugar claims in `stdlib/kernel.ev` wrap `LibCall` —
adding a syscall = adding a `BuildXyz` claim, never new grammar.
Single-writer rule for `effects`: one unconditional `effects = …`, or
multiple guarded `cond ⇒ effects = …`; multi-writer composition is
`effects = a ++ b ++ c`. `is_first_tick ∈ Bool` is the tick-0
discriminator; `_x` names are prev-tick carries.

### 2.9 Booleans and precedence (correct spellings)

- `true` / `false` lowercase — `True` parses as an unbound name and the
  constraint **silently drops** (§4 V2).
- `⇒` binds tighter than `∧`: wrap compound consequents `A ⇒ (B ∧ C)`.
- `=` binds tighter than `∧`/`∨`/comparisons: wrap boolean assignments
  `flag = (x < 5 ∧ y > 0)`.

---

## 3. The preference hierarchy — X over Y

3.1 **Element iteration over index ranges.** `∀ x ∈ seq : …` over
`∀ i ∈ {0..#seq-1} : … seq[i] …`. The index range survives ONLY where
the position IS the predicate's subject: allocation cursors
(`alloc ∧ _cur = k`), wire positions passed to claims (`i ↦ k`),
positional-parameter slots, order-sensitive folds. Reason: a quantifier
over positions claims the *positions* matter; if only the elements do,
the surface is lying about the data.

3.2 **The registry doctrine: allocate by position, everything else by
key.** Placing a new entry is choosing one slot among identical
empties — that is what a cursor is, and the one honest positional
operation. Updates and reads key on a unique field. Never store an
index as FSM state to identify an entry — store the key
(`setvar_cur_name ∈ String`, not `setvar_cur ∈ Int`); an index-valued
name lookup is the index-in-interface idiom (§4 V6) — write a keyed
projection.

3.3 **Record types over parallel Seqs; domain types in interfaces.**
If two Seqs are "supposed to align," that is a record type
(`type Edge(from, to ∈ Int)` + `edges ∈ Seq(Edge)`). If a claim's
interface uses `Int` indices to identify "which item," an
implementation choice is leaking — domain types in, domain types out.
(Exception: `coindexed` parallel Seqs remain the documented workaround
where a lowering gap blocks record-element access on an *unbounded*
Seq — a gap workaround, to be retired with the gap, never a design.)

3.4 **Pin-pair projections over ternary chains — and the ternary
ruling.** The blessed/dispreferred line:

- **Blessed — the carried-write hold chain.** The single covering
  assignment of a carried field, guards in priority order, final arm
  holding the prev-tick value:

  ```evident
  match_st = (is_first_tick ? 0
      : enter_match ? 0
      : (scrut_lastres ∨ scrut_ident) ? 1
      : _match_st)
  ```

  This is the FSM transition relation in prioritized-guard form. The
  kernel requires exactly one covering write per carried field, the
  guards are *events*, the chain terminates in the hold — there is no
  keyed projection to prefer because nothing is being looked up. Also
  blessed: the single conditional `cond ? a : b` (one test is an ite,
  not a chain), and capture-or-carry views (`x_now = (cap ? new : _x)`).

- **Dispreferred (WARN) — value-selection chains.** Two or more tests
  comparing a discriminant against successive keys or case codes to
  *select among values*:

  ```evident
  fold_tester1 = (fold_ctor1 = "IntResult" ? z_intres_test
      : fold_ctor1 = "StringResult" ? z_strres_test
      : fold_tester1_user)
  ```

  This is a keyed lookup spelled as the lowered covered-chain artifact.
  The set-theoretic surface is the keyed-projection pin pair over a
  registry (when the table is data) or `match` over an enum (when the
  discriminant is a case code). If the table's entries are not yet
  representable as a registry (floor handles, scalar lookups), the
  chain is tolerated with a WARN — the durable fix is a lowering or
  registry change, not acceptance of the chain (§1.5).

3.5 **Composition: shortest form that works.** Bare/call for
components (internals hidden); `(slot ↦ value)` when names don't
already agree; `..` ONLY for deliberate context sharing; `v ∈ Type`
receiver instances for zero implicit sharing. A `..`-lift reached for
because "the names happen to line up" is a component boundary erased
(§4 V11).

3.6 **Naming.** Meaningful 2–3-word snake_case (`fetch_end`,
`win_avail`, `match_scrut_idx`). No letter-code prefixes (`ed_`,
`rc_`, `bcast_`) — hand-namespacing is a *symptom of missing scoping*,
not a style (the claim-headers plan names it as exactly that; the
1,316-name rename of `docs/rename-map.md` is the repo-wide repayment of
that debt). Expression-scoped bound variables may be short (`e`, `v`,
`k`, `x`).

3.7 **Comments: only the five allowed classes.** Module contract
headers (`-- MODULE X` with CONSUMES/PRODUCES/MAINTAINS); measured
traps with measurement and date; cross-file encoding/wire facts;
one-line section banners; test headers (`-- entry:` / `-- expect:`).
Never: restating the next line, history/narration, code examples
inside prose comments (text-level transforms parse flattened source —
quoted code in comments has caused real false positives), or standard
language semantics.

3.8 **Booleans/precedence: the correct spellings of §2.9.** Lowercase
`true`/`false`; wrapped consequents; wrapped boolean assignments.
These are not style — the misspellings are silently vacuous (§4 V2).

---

## 4. The violation catalog

Severities: **BLOCKER** = not Evident, silently vacuous, or a direct
operator-ruling violation. **WARN** = dispreferred form with a real
preferred alternative. **NOTE** = style.

| # | Violation | Severity | Precedent |
|---|-----------|----------|-----------|
| V1 | **Invented grammar laundered through transforms** — a surface form outside §2, made to "work" by teaching a pre-oracle pass to lower it | BLOCKER | `∀ (k, e) ∈ xs` indexed-family form: invented + lowered in `d1be22a` with byte-identical output and green gates; reverted as invalid grammar in `2b0efb2` (2026-06-09). The surface implied xs contains (index, element) tuples — false. |
| V2 | **Silently-vacuous constructions** — any construct the frozen oracle drops without error: Seq membership `x ∈ xs`; capitalized `True`/`False`; any unbound name standing as a constraint; unwrapped `A ⇒ B ∧ C` / `flag = a ∧ b` (precedence makes them assert something else) | BLOCKER | CLAUDE.md footgun boxes; `scripts/passes/` lint for Seq membership. The language's failure mode is SILENT — a constraint whose misspelling vanishes quietly (exit 0, vacuously SAT) deserves extra suspicion, so every near-miss of these spellings gets flagged. |
| V3 | **Surface-reverted-to-encoding** — rewriting a blessed surface form into the lowered/fast encoding (Seq forms → numbered scalars, element-∀ → hand-unrolled lines, pin pair → hand-written select chain) to dodge a lowering gap or perf wall | BLOCKER | Operator ruling §1.5; `post-cutover-roadmap.md` resolution 2026-06-09 ("the surface is free"). |
| V4 | **Function-shaped constructs** — value-returning call forms, `find(...)`-likes, any grammar where a claim is "called for" a value rather than composed | BLOCKER | The rejected `find(...)` proposal (operator ruling, §1.1). |
| V5 | **Hand-namespace prefixes** on new names (`bcast_on`, `mp_st`) | WARN | `claim-headers-interface.md`: "people namespace manually when scoping is dynamic"; the rename-map purge. |
| V6 | **Index-in-interface / index-as-state** — `Int` indices identifying "which item" in a claim interface, or an index stored as FSM state to identify a registry entry | WARN | CLAUDE.md registry doctrine (`setvar_cur_name`, not `setvar_cur`). |
| V7 | **Anemic types** — bodyless `type T(a, b)` where a field relationship exists to state | WARN | CLAUDE.md "a type is its constraints"; `FtiBuffer` as the model. |
| V8 | **God-records** — wide carried records accreting unrelated fields (every field needs a covering write every tick; width is the tell that the type is not one set) | WARN | CLAUDE.md carry guardrails ("this also forbids wide god-records"). |
| V9 | **Ternary value-selection chains in source** (≥2 key/code tests selecting among values) — distinguish from the blessed hold chain, §3.4 | WARN | Operator ruling §1.4; `driver_matchpin.ev` floor-dispatch chains are the live instance. |
| V10 | **Parallel Seqs** meant to align | WARN | CLAUDE.md idioms-to-avoid; `seq-bounded-catalog.md` A3 marks the coindexed workaround as gap-driven. |
| V11 | **`..`-lift where a component boundary was meant** — lifting for incidental name agreement rather than deliberate context sharing | WARN | `6af4042` (bare hides, `..` lifts; conformance 139/140/141); headers plan rule 4 (wide-context drivers keep `..` legitimately until context bundles exist). |
| V12 | **Range-of-index `∀` where the element form works** (position is not the subject) | WARN | CLAUDE.md registry doctrine + idioms-to-avoid. |
| V13 | **Comment violations** — restating, narration, code-in-prose, semantics explanations | NOTE | CLAUDE.md comment rules (code-in-comments caused real lint false positives). |
| V14 | **Naming violations** — letter-code prefixes aside (V5), opaque abbreviations where 2–3 words exist | NOTE | `docs/rename-map.md`. |
| V15 | **Unbounded carried collections** — a carried `Seq`/cons list with no static bound, or unbounded data kept in state instead of on the tape | BLOCKER | Operator ruling §1.2 (FSMs stay finite). |

---

## 5. The "is this real Evident?" test

For any PROPOSED grammar, surface form, or transform rule:

1. **Catalog check.** Is it in §2, CLAUDE.md's spec, a conformance
   fixture, or an operator-approved plan (e.g.
   `claim-headers-interface.md`)? If not, it is new grammar.
2. **The truth test (the tuple-bind test).** Does the surface tell the
   truth about the data? `∀ (k, e) ∈ xs` failed this: it asserts pairs
   that do not exist. A binder must bind what the collection actually
   contains; a membership must be a membership; an `=` must constrain.
3. **The set-theory test.** Does it read as set theory / logic over
   named sets, or as an operational recipe?
4. **The mathematician test.** Would a mathematician parse it correctly
   *without knowing the implementation*? (A reader of `coindexed(a, b)`
   correctly infers a zip; a reader of `(k, e) ∈ xs` infers tuples in
   xs — wrongly.)
5. **The function test.** Does it add a function-shaped thing — a call
   that yields a value? (§1.1.)

**New grammar requires an explicit operator ruling.** The critic's job
on a new construction is to FLAG it (`requires operator ruling`),
never to approve it — even when it passes tests 2–5, and *especially*
when a transform already lowers it and the gates are green (the
admissibility rule). Calibration for this document lives in
`docs/evident-purism-calibration.md`.
